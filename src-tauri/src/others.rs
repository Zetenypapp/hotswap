//! Games outside Steam, Epic and GOG: games the user adds by their exe, and Minecraft: Java Edition, from whichever launcher is
//! installed (Modrinth App, the official launcher, Prism, CurseForge). It runs as Java, so the
//! watcher spots it by its Java command line (`is_minecraft`), not by an install folder.

use crate::steam::{Art, Game, Launch};
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub const MINECRAFT: &str = "minecraft";

/// A launcher's exe, from Windows' list of installed programs.
fn installed_exe(name: &str) -> Option<PathBuf> {
    use winreg::{enums::*, RegKey};
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    let roots = [(HKEY_CURRENT_USER, KEY.to_string()), (HKEY_LOCAL_MACHINE, KEY.to_string()), (HKEY_LOCAL_MACHINE, KEY.replace("Software", r"Software\WOW6432Node"))];
    roots.iter().find_map(|(hive, path)| {
        let list = RegKey::predef(*hive).open_subkey(path).ok()?;
        list.enum_keys().flatten().find_map(|k| {
            let app = list.open_subkey(k).ok()?;
            let shown: String = app.get_value("DisplayName").ok()?;
            if !shown.starts_with(name) {
                return None;
            }
            let icon: String = app.get_value("DisplayIcon").ok()?;
            let exe = PathBuf::from(icon.split(',').next()?.trim_matches('"'));
            exe.is_file().then_some(exe)
        })
    })
}

/// When Minecraft last ran: the newest `logs\latest.log` of any launcher's profiles.
fn last_played(appdata: &PathBuf) -> u64 {
    let home = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
    let mut logs = vec![appdata.join(".minecraft")];
    for (dir, sub) in [(appdata.join(r"ModrinthApp\profiles"), ""), (appdata.join(r"PrismLauncher\instances"), ".minecraft"), (home.join(r"curseforge\minecraft\Instances"), "")] {
        logs.extend(fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path().join(sub)));
    }
    logs.iter()
        .filter_map(|d| fs::metadata(d.join(r"logs\latest.log")).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .max()
        .unwrap_or(0)
}

pub fn scan() -> Vec<Game> {
    let mut games = custom_games();
    games.extend(minecraft());
    games
}

fn minecraft() -> Vec<Game> {
    let appdata = PathBuf::from(std::env::var("APPDATA").unwrap_or_default());
    let launcher = ["Modrinth App", "Minecraft Launcher", "Prism Launcher", "CurseForge"].iter().find_map(|n| installed_exe(n));
    let played = last_played(&appdata);
    if launcher.is_none() && played == 0 {
        return vec![];
    }
    vec![Game {
        id: MINECRAFT.into(),
        source: "other",
        app_id: 0,
        name: "Minecraft: Java Edition".into(),
        size_on_disk: 0,
        last_played: played,
        playtime: 0,
        kind: "game",
        steam_type: None,
        anticheat: Vec::new(),
        engine: None,
        single_player: false,
        build: String::new(),
        art: Art::default(),
        install: PathBuf::new(),
        launch: launcher.map(|p| Launch::Exe { dir: p.parent().map(PathBuf::from).unwrap_or_default(), path: p }).unwrap_or_default(),
    }]
}

/// Minecraft's game client: Java running Minecraft (vanilla, Fabric, Quilt, Forge or NeoForge).
/// Only the client gets `--assetIndex`, so a server on this PC never counts.
pub fn is_minecraft(p: &sysinfo::Process) -> bool {
    let name = p.name().to_string_lossy().to_lowercase();
    (name == "javaw.exe" || name == "java.exe")
        && p.cmd().iter().any(|a| {
            let a = a.to_string_lossy();
            a == "--assetIndex" || a.ends_with("KnotClient") || a == "net.minecraft.client.main.Main"
        })
}

/* ---------- Games the user adds by picking their exe (Xbox, EA, Ubisoft, Battle.net, itch.io...) ---------- */

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Custom {
    pub id: String,
    pub name: String,
    pub exe: PathBuf,
}

/// custom_games.json in HotSwap's data folder (%APPDATA%\<identifier in tauri.conf.json>).
fn custom_file() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("app.hotswap.desktop").join("custom_games.json")
}

pub fn customs() -> Vec<Custom> {
    fs::read_to_string(custom_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_customs(list: &[Custom]) -> Result<(), String> {
    let path = custom_file();
    fs::create_dir_all(path.parent().ok_or("No data folder")?).map_err(|e| e.to_string())?;
    fs::write(&path, serde_json::to_string_pretty(list).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// The game's folder counts as the game: anything running from it is "the game", and Close game ends
/// everything in it. So it must be the game's own folder, never Windows, Program Files, a drive or a
/// folder full of other things like Downloads or the Desktop.
fn own_folder(exe: &Path) -> Result<PathBuf, String> {
    let dir = exe.parent().ok_or("No folder")?.to_path_buf();
    let low = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    let d = low(&dir);
    if dir.parent().is_none() || d.len() <= 3 {
        return Err("The game needs its own folder, not a whole drive".into());
    }
    let env = |k: &str| std::env::var(k).ok().map(PathBuf::from);
    let home = env("USERPROFILE");
    let never_inside = [env("WINDIR")];
    let never_equal = [
        env("ProgramFiles"), env("ProgramFiles(x86)"), env("ProgramData"), env("LOCALAPPDATA"), env("APPDATA"),
        home.clone(), home.as_ref().map(|h| h.join("Desktop")), home.as_ref().map(|h| h.join("Downloads")),
        home.as_ref().map(|h| h.join("Documents")), env("TEMP"),
    ];
    if never_inside.iter().flatten().any(|w| d == low(w) || d.starts_with(&(low(w) + "\\"))) {
        return Err("Windows' own programs can't be added".into());
    }
    if never_equal.iter().flatten().any(|f| d == low(f)) {
        return Err("Put the game in a folder of its own first: everything running from its folder counts as the game".into());
    }
    Ok(dir)
}

/// "RocketLeague" or "rocket_league" from the exe's name, as "Rocket League".
fn nice_name(exe: &Path) -> String {
    let stem = exe.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut out = String::new();
    let mut prev = ' ';
    for c in stem.chars() {
        let c = if c == '_' || c == '-' || c == '.' { ' ' } else { c };
        if c.is_uppercase() && prev.is_lowercase() {
            out.push(' ');
        }
        if !(c == ' ' && prev == ' ') {
            out.push(c);
        }
        prev = c;
    }
    out.trim().to_string()
}

fn custom_games() -> Vec<Game> {
    customs().into_iter().filter(|c| c.exe.is_file()).filter_map(|c| {
        let install = own_folder(&c.exe).ok()?;
        let art = art_of(&c.id).map(|p| Art::picture(&p)).unwrap_or_default();
        Some(Game {
            id: c.id,
            source: "custom",
            app_id: 0,
            name: c.name,
            size_on_disk: 0,
            last_played: fs::metadata(&c.exe).ok()?.accessed().ok()?.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            playtime: 0,
            kind: "game",
            steam_type: None,
            anticheat: crate::steam::detect_anticheat(&Default::default(), &install),
            engine: crate::engines::detect(&install),
            single_player: false,
            build: String::new(),
            art,
            launch: Launch::Exe { path: c.exe.clone(), dir: install.clone() },
            install,
        })
    }).collect()
}

/// Pictures for games you added: custom_art\<id>-<time>.<ext> next to custom_games.json (the time
/// makes a new picture a new address, so the window doesn't show the cached old one).
pub fn art_dir() -> PathBuf {
    custom_file().with_file_name("custom_art")
}

fn art_key(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect::<String>() + "-"
}

fn art_of(id: &str) -> Option<PathBuf> {
    let key = art_key(id);
    let mut found: Vec<PathBuf> = fs::read_dir(art_dir()).ok()?.flatten().map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&key)))
        .collect();
    found.sort();
    found.pop()
}

/// Asks for a picture for a game you added and keeps a copy. True if one was picked.
#[tauri::command]
pub async fn custom_game_art(app: tauri::AppHandle, id: String) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    if !customs().iter().any(|c| c.id == id) {
        return Err("Not a game you added".into());
    }
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("Choose a picture").add_filter("Pictures", &["png", "jpg", "jpeg", "webp", "gif"]).blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(src) = picked.and_then(|p| p.into_path().ok()) else { return Ok(false) };
    let ext = src.extension().map(|x| x.to_string_lossy().to_lowercase()).filter(|x| ["png", "jpg", "jpeg", "webp", "gif"].contains(&x.as_str())).ok_or("That isn't a picture")?;
    if fs::metadata(&src).map_err(|e| e.to_string())?.len() > 20 << 20 {
        return Err("The picture is over 20 MB".into());
    }
    let dir = art_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let old = art_of(&id);
    let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    fs::copy(&src, dir.join(format!("{}{now}.{ext}", art_key(&id)))).map_err(|e| e.to_string())?;
    if let Some(o) = old {
        let _ = fs::remove_file(o);
    }
    Ok(true)
}

/// Asks for a game's exe and adds it. Returns the new game's ID, or None if nothing was picked.
#[tauri::command]
pub async fn custom_game_add(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("Choose the game's .exe").add_filter("Games", &["exe"]).blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(exe) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    if !exe.is_file() || !exe.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
        return Err("That isn't a program".into());
    }
    own_folder(&exe)?;
    let mut list = customs();
    let key = exe.to_string_lossy().to_lowercase();
    if let Some(c) = list.iter().find(|c| c.exe.to_string_lossy().to_lowercase() == key) {
        return Ok(Some(c.id.clone()));
    }
    use sha2::Digest;
    let id = format!("custom:{}", &format!("{:x}", sha2::Sha256::digest(key.as_bytes()))[..12]);
    list.push(Custom { id: id.clone(), name: nice_name(&exe), exe });
    save_customs(&list)?;
    Ok(Some(id))
}

#[tauri::command]
pub fn custom_game_remove(id: String) -> Result<(), String> {
    if let Some(p) = art_of(&id) {
        let _ = fs::remove_file(p);
    }
    let list: Vec<Custom> = customs().into_iter().filter(|c| c.id != id).collect();
    save_customs(&list)
}

#[tauri::command]
pub fn custom_game_rename(id: String, name: String) -> Result<(), String> {
    let name: String = name.trim().chars().take(80).collect();
    if name.is_empty() {
        return Err("Give it a name".into());
    }
    let mut list = customs();
    list.iter_mut().filter(|c| c.id == id).for_each(|c| c.name = name.clone());
    save_customs(&list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_folders() {
        assert_eq!(nice_name(Path::new(r"C:\Games\RL\RocketLeague.exe")), "Rocket League");
        assert_eq!(nice_name(Path::new(r"C:\Games\x\cool_game-v2.exe")), "cool game v2");
        assert!(own_folder(Path::new(r"C:\game.exe")).is_err());
        let win = std::env::var("WINDIR").unwrap();
        assert!(own_folder(&PathBuf::from(win).join(r"System32\notepad.exe")).is_err());
        let home = std::env::var("USERPROFILE").unwrap();
        assert!(own_folder(&PathBuf::from(&home).join(r"Downloads\setup.exe")).is_err());
        assert!(own_folder(&PathBuf::from(&home).join(r"Downloads\MyGame\game.exe")).is_ok());
    }
}
