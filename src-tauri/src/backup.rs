//! Save backups: when a session with mods starts, the game's save folder is zipped into HotSwap's
//! own folder (backups\<game>\<unix time>.zip), keeping the newest few. The game's folder is only read.

use crate::watch;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};

const KEEP: usize = 5;
const MAX_BYTES: u64 = 2 << 30;

/// Documents, wherever Windows has put it (e.g. OneDrive), from the registry.
fn documents() -> Option<String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
        .and_then(|k| k.get_value("Personal"))
        .ok()
        .or_else(|| Some(format!(r"{}\Documents", std::env::var("USERPROFILE").ok()?)))
}

/// A save folder: a picked folder, or a template with `%LOCALAPPDATA%`, `%APPDATA%`, `%USERPROFILE%`,
/// `%DOCUMENTS%` or `<install>`. None unless it exists and isn't a drive root.
fn resolve(template: &str, install: Option<PathBuf>) -> Option<PathBuf> {
    let mut s = template.to_string();
    if s.contains("<install>") {
        s = s.replace("<install>", install?.to_str()?);
    }
    for var in ["LOCALAPPDATA", "APPDATA", "USERPROFILE"] {
        let k = format!("%{var}%");
        if s.contains(&k) {
            s = s.replace(&k, &std::env::var(var).ok()?);
        }
    }
    if s.contains("%DOCUMENTS%") {
        s = s.replace("%DOCUMENTS%", &documents()?);
    }
    let p = PathBuf::from(s);
    (p.is_absolute() && p.parent().is_some() && p.is_dir()).then_some(p)
}

fn store(app: &AppHandle, game: &str) -> Result<PathBuf, String> {
    let safe: String = game.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("backups").join(safe))
}

/// Every file under `dir` (links are never followed out of it), stopping past MAX_BYTES.
fn walk(dir: &Path, out: &mut Vec<PathBuf>, total: &mut u64) -> Result<(), String> {
    for e in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            walk(&e.path(), out, total)?;
        } else if ft.is_file() {
            *total += e.metadata().map(|m| m.len()).unwrap_or(0);
            if *total > MAX_BYTES {
                return Err("The save folder is over 2 GB, too big to back up".into());
            }
            out.push(e.path());
        }
    }
    Ok(())
}

fn zip_dir(src: &Path, dest: &Path) -> Result<u64, String> {
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};
    let (mut list, mut total) = (Vec::new(), 0);
    walk(src, &mut list, &mut total)?;
    if list.is_empty() {
        return Err("The save folder is empty".into());
    }
    let tmp = dest.with_extension("part");
    let mut z = ZipWriter::new(fs::File::create(&tmp).map_err(|e| e.to_string())?);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).compression_level(Some(1)).large_file(true);
    let res = (|| {
        for p in &list {
            // A save the game is writing right now may be locked: skip that one file, keep the rest.
            let Ok(mut f) = fs::File::open(p) else { continue };
            let name = p.strip_prefix(src).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
            z.start_file(name, opts).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, &mut z).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
        fs::rename(&tmp, dest).map_err(|e| e.to_string())
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res.map(|_| total)
}

/// The backups kept for a game, newest first: [{ at (unix s), bytes }].
fn list(dir: &Path) -> Vec<(u64, u64)> {
    let mut v: Vec<(u64, u64)> = fs::read_dir(dir).into_iter().flatten().flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            Some((name.strip_suffix(".zip")?.parse().ok()?, e.metadata().ok()?.len()))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v
}

/// Backs up a game's saves now. `path` is a save-folder template or a folder the user picked.
#[tauri::command]
pub async fn save_backup(app: AppHandle, watch: State<'_, watch::Watch>, game: String, path: String) -> Result<Value, String> {
    let src = resolve(&path, watch.install_of(&game)).ok_or("No save folder found")?;
    let dir = store(&app, &game)?;
    tauri::async_runtime::spawn_blocking(move || {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
        let bytes = zip_dir(&src, &dir.join(format!("{at}.zip")))?;
        for (old, _) in list(&dir).into_iter().skip(KEEP) {
            let _ = fs::remove_file(dir.join(format!("{old}.zip")));
        }
        Ok(json!({ "at": at, "bytes": bytes }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Unpacks a backup into `dest` (new), refusing entries that would land outside it.
fn unzip(zip: &Path, dest: &Path) -> Result<(), String> {
    let mut z = zip::ZipArchive::new(fs::File::open(zip).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { continue };
        let out = dest.join(rel);
        if f.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::io::copy(&mut f, &mut fs::File::create(&out).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Puts a backup back: the saves there now are backed up first, the backup is unpacked next to the save
/// folder, and only then swapped in, so a failure never leaves half a folder. Refused while the game runs.
fn restore(src: &Path, dir: &Path, at: u64) -> Result<(), String> {
    let zip = dir.join(format!("{at}.zip"));
    if !zip.is_file() {
        return Err("That backup is gone".into());
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs().max(at + 1);
    match zip_dir(src, &dir.join(format!("{now}.zip"))) {
        Ok(_) => {}
        Err(e) if e == "The save folder is empty" => {}
        Err(e) => return Err(format!("Couldn't back up the saves there now first: {e}")),
    }
    let name = src.file_name().ok_or("Bad save folder")?.to_string_lossy().to_string();
    let fresh = src.with_file_name(format!("{name}.hotswap-restore"));
    let old = src.with_file_name(format!("{name}.hotswap-old"));
    let _ = fs::remove_dir_all(&fresh);
    let _ = fs::remove_dir_all(&old);
    let res = unzip(&zip, &fresh).and_then(|_| fs::rename(src, &old).map_err(|e| format!("The save folder is in use ({e})")));
    if let Err(e) = res {
        let _ = fs::remove_dir_all(&fresh);
        return Err(e);
    }
    if let Err(e) = fs::rename(&fresh, src) {
        let _ = fs::rename(&old, src);
        let _ = fs::remove_dir_all(&fresh);
        return Err(e.to_string());
    }
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

#[tauri::command]
pub async fn save_backup_restore(app: AppHandle, watch: State<'_, watch::Watch>, game: String, path: String, at: u64) -> Result<(), String> {
    if watch.running().as_deref() == Some(game.as_str()) {
        return Err("Close the game first".into());
    }
    let src = resolve(&path, watch.install_of(&game)).ok_or("No save folder found")?;
    let dir = store(&app, &game)?;
    tauri::async_runtime::spawn_blocking(move || restore(&src, &dir, at)).await.map_err(|e| e.to_string())?
}

/// Whether the save folder exists, and the backups kept.
#[tauri::command]
pub fn save_backups(app: AppHandle, watch: State<'_, watch::Watch>, game: String, path: Option<String>) -> Value {
    let found = path.and_then(|p| resolve(&p, watch.install_of(&game))).map(|p| p.to_string_lossy().to_string());
    let kept = store(&app, &game).map(|d| list(&d)).unwrap_or_default();
    json!({ "folder": found, "list": kept.iter().map(|(at, bytes)| json!({ "at": at, "bytes": bytes })).collect::<Vec<_>>() })
}

/// Opens the game's backups folder in Explorer.
#[tauri::command]
pub fn save_backup_open(app: AppHandle, game: String) -> Result<(), String> {
    let dir = store(&app, &game)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer").arg(&dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Asks for a game's save folder.
#[tauri::command]
pub async fn save_backup_pick(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_title("Choose the game's save folder").blocking_pick_folder())
        .await
        .ok()??;
    picked.into_path().ok().map(|p| p.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zips_and_resolves() {
        let base = std::env::temp_dir().join(format!("hs-backup-test-{}", std::process::id()));
        let src = base.join("saves");
        fs::create_dir_all(src.join("slot1")).unwrap();
        fs::write(src.join("slot1").join("a.sav"), b"hello").unwrap();
        assert_eq!(resolve(src.to_str().unwrap(), None), Some(src.clone()));
        assert_eq!(resolve("<install>\\saves", Some(base.clone())), Some(src.clone()));
        assert_eq!(resolve("C:\\", None), None);
        assert_eq!(zip_dir(&src, &base.join("1.zip")).unwrap(), 5);
        assert_eq!(list(&base), vec![(1, fs::metadata(base.join("1.zip")).unwrap().len())]);
        // Restore: the folder ends up exactly as backed up, and what was there is backed up too.
        fs::write(src.join("slot1").join("a.sav"), b"changed").unwrap();
        fs::write(src.join("new.sav"), b"x").unwrap();
        restore(&src, &base, 1).unwrap();
        assert_eq!(fs::read(src.join("slot1").join("a.sav")).unwrap(), b"hello");
        assert!(!src.join("new.sav").exists());
        assert_eq!(list(&base).len(), 2);
        let _ = fs::remove_dir_all(&base);
    }
}
