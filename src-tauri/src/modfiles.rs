//! Mod manager for Unreal games modded the UE4SS way (Subnautica 2): UE4SS itself, its Lua mods
//! (Binaries/Win64/ue4ss/Mods/<Name>, switched on by an enabled.txt), and .pak mods in
//! Content/Paks/~mods and Content/Paks/LogicMods. Mods come from Nexus Mods: browsing uses the
//! public API, downloading happens on the website, and HotSwap installs the downloaded archive.

use serde::Serialize;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// UE4SS ships these mods itself.
const BUILT_IN: [&str; 10] = ["BPModLoaderMod", "BPML_GenericFunctions", "CheatManagerEnablerMod", "ConsoleCommandsMod", "ConsoleEnablerMod", "Keybinds", "LineTraceMod", "SplitScreenMod", "jsbLuaProfilerMod", "shared"];

#[derive(Serialize)]
pub struct FileMod {
    /// Stable ID: "<kind>:<name>".
    id: String,
    name: String,
    /// "ue4ss", "lua", "pak" or "logic".
    kind: &'static str,
    enabled: bool,
    built_in: bool,
}

pub struct Game {
    /// The Steam install folder.
    pub root: PathBuf,
    /// The Unreal project folder inside it, e.g. "Subnautica2".
    pub project: &'static str,
}

impl Game {
    fn proj(&self) -> PathBuf {
        self.root.join(self.project)
    }
    fn win64(&self) -> PathBuf {
        self.proj().join("Binaries").join("Win64")
    }
    fn lua_dir(&self) -> PathBuf {
        self.win64().join("ue4ss").join("Mods")
    }
    fn pak_dir(&self, kind: &str, enabled: bool) -> PathBuf {
        let name = match kind { "logic" => "LogicMods", _ => "~mods" };
        self.proj().join("Content").join("Paks").join(if enabled { name.to_string() } else { format!("{name} (disabled by Hotswap)") })
    }

    pub fn list(&self) -> Vec<FileMod> {
        let mut out = Vec::new();
        if self.win64().join("ue4ss").is_dir() {
            out.push(FileMod { id: "ue4ss:UE4SS".into(), name: "UE4SS (mod loader)".into(), kind: "ue4ss", enabled: true, built_in: false });
        }
        if let Ok(rd) = fs::read_dir(self.lua_dir()) {
            for e in rd.flatten().filter(|e| e.path().is_dir()) {
                let name = e.file_name().to_string_lossy().to_string();
                let enabled = e.path().join("enabled.txt").exists();
                out.push(FileMod { id: format!("lua:{name}"), built_in: BUILT_IN.contains(&name.as_str()), name, kind: "lua", enabled });
            }
        }
        for kind in ["pak", "logic"] {
            for enabled in [true, false] {
                let Ok(rd) = fs::read_dir(self.pak_dir(kind, enabled)) else { continue };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pak")) {
                        let name = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
                        out.push(FileMod { id: format!("{kind}:{name}"), name, kind, enabled, built_in: false });
                    }
                }
            }
        }
        out.sort_by(|a, b| (a.built_in, a.kind != "ue4ss", a.name.to_lowercase()).cmp(&(b.built_in, b.kind != "ue4ss", b.name.to_lowercase())));
        out
    }

    /// The files of a .pak mod: the .pak plus its .ucas/.utoc/.sig companions.
    fn pak_files(&self, kind: &str, name: &str, enabled: bool) -> Vec<PathBuf> {
        let dir = self.pak_dir(kind, enabled);
        ["pak", "ucas", "utoc", "sig"].iter().map(|x| dir.join(format!("{name}.{x}"))).filter(|p| p.exists()).collect()
    }

    pub fn set_enabled(&self, id: &str, on: bool) -> Result<(), String> {
        let (kind, name) = split_id(id)?;
        match kind {
            "lua" => {
                let flag = self.lua_dir().join(name).join("enabled.txt");
                if on { fs::write(flag, "").map_err(|e| e.to_string()) } else { fs::remove_file(flag).or_else(ignore_missing) }
            }
            "pak" | "logic" => {
                let to = self.pak_dir(kind, on);
                fs::create_dir_all(&to).map_err(|e| e.to_string())?;
                for f in self.pak_files(kind, name, !on) {
                    fs::rename(&f, to.join(f.file_name().unwrap())).map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            _ => Err("UE4SS can't be turned off here; remove it instead".into()),
        }
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let (kind, name) = split_id(id)?;
        match kind {
            "lua" => fs::remove_dir_all(self.lua_dir().join(name)).map_err(|e| e.to_string()),
            "pak" | "logic" => {
                for on in [true, false] {
                    for f in self.pak_files(kind, name, on) {
                        fs::remove_file(f).map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            }
            "ue4ss" => {
                // UE4SS: its folder and the proxy DLL that loads it.
                let w = self.win64();
                fs::remove_dir_all(w.join("ue4ss")).map_err(|e| e.to_string())?;
                for dll in ["dwmapi.dll", "UE4SS-settings.ini"] {
                    let _ = fs::remove_file(w.join(dll));
                }
                Ok(())
            }
            _ => Err("Unknown mod".into()),
        }
    }

    /// Installs a downloaded mod archive (.zip or .7z) or a bare .pak. Returns what was installed.
    pub fn install(&self, file: &Path, scratch: &Path) -> Result<String, String> {
        let ext = file.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        let tmp = scratch.join("unpack");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        match ext.as_str() {
            "pak" => {
                fs::copy(file, tmp.join(file.file_name().unwrap())).map_err(|e| e.to_string())?;
            }
            "zip" => unzip(file, &tmp)?,
            "7z" => sevenz_rust::decompress_file(file, &tmp).map_err(|e| format!("7z: {e}"))?,
            "rar" => return Err("RAR archives aren't supported yet. Pick the .zip or .7z download if the mod has one.".into()),
            _ => return Err("Pick a .zip, .7z or .pak file".into()),
        }
        let result = self.place(&tmp);
        let _ = fs::remove_dir_all(&tmp);
        result
    }

    /// Works out where an unpacked mod belongs and copies it there.
    fn place(&self, tmp: &Path) -> Result<String, String> {
        let files = walk(tmp);
        let rel = |p: &Path| p.strip_prefix(tmp).unwrap().to_path_buf();
        let lower = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
        let proj = self.project.to_lowercase();

        // 1. Laid out like the game folder: "<Project>/..." or "Binaries/..." / "Content/...".
        for f in &files {
            let l = lower(&rel(f));
            for anchor in [format!("{proj}/binaries/"), format!("{proj}/content/"), "binaries/win64/".into(), "content/paks/".into()] {
                if let Some(i) = l.find(&anchor) {
                    let base = tmp.join(&rel(f).to_string_lossy()[..i]);
                    let dest = if anchor.starts_with(&proj) { self.root.clone() } else { self.proj() };
                    copy_tree(&base, &dest)?;
                    return Ok(name_of(tmp));
                }
            }
        }
        // 2. UE4SS itself: its loader DLL and folder go next to the game executable.
        if let Some(dll) = files.iter().find(|f| {
            let n = f.file_name().unwrap().to_string_lossy().to_lowercase();
            n == "dwmapi.dll" || n == "ue4ss.dll"
        }) {
            let base = if dll.parent().is_some_and(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("ue4ss"))) {
                dll.parent().unwrap().parent().unwrap().to_path_buf()
            } else {
                dll.parent().unwrap().to_path_buf()
            };
            copy_tree(&base, &self.win64())?;
            return Ok("UE4SS".into());
        }
        // 3. A UE4SS Lua mod: a folder with Scripts/main.lua.
        if let Some(main) = files.iter().find(|f| lower(&rel(f)).ends_with("scripts/main.lua")) {
            let dir = main.parent().and_then(|p| p.parent()).unwrap();
            let name = if dir == tmp { name_of(tmp) } else { dir.file_name().unwrap().to_string_lossy().to_string() };
            let dest = self.lua_dir().join(&name);
            copy_tree(dir, &dest)?;
            let _ = fs::write(dest.join("enabled.txt"), "");
            return Ok(name);
        }
        // 4. Pak mods: Blueprint ("LogicMods") ones by their folder name, the rest go to ~mods.
        let paks: Vec<&PathBuf> = files.iter().filter(|f| {
            let x = f.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            ["pak", "ucas", "utoc", "sig"].contains(&x.as_str())
        }).collect();
        if !paks.is_empty() {
            for p in &paks {
                let kind = if lower(&rel(p)).contains("logicmods") { "logic" } else { "pak" };
                let dir = self.pak_dir(kind, true);
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                fs::copy(p, dir.join(p.file_name().unwrap())).map_err(|e| e.to_string())?;
            }
            return Ok(paks[0].file_stem().unwrap().to_string_lossy().to_string());
        }
        Err("Couldn't tell where this mod goes. It may need to be installed by hand; see its Nexus page.".into())
    }
}

fn split_id(id: &str) -> Result<(&str, &str), String> {
    let (k, n) = id.split_once(':').ok_or("Bad mod id")?;
    // Names come from folder and file names; never let one climb out of its folder.
    if n.is_empty() || n.contains(['/', '\\']) || n.contains("..") {
        return Err("Bad mod name".into());
    }
    Ok((k, n))
}

fn ignore_missing(e: std::io::Error) -> Result<(), String> {
    if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e.to_string()) }
}

/// A readable name for an unpacked archive: its single top folder, else "Mod".
fn name_of(tmp: &Path) -> String {
    let dirs: Vec<_> = fs::read_dir(tmp).map(|r| r.flatten().collect()).unwrap_or_default();
    match dirs.as_slice() {
        [one] if one.path().is_dir() => one.file_name().to_string_lossy().to_string(),
        _ => "Mod".into(),
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() { stack.push(p) } else { out.push(p) }
        }
    }
    out.sort();
    out
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    for f in walk(from) {
        let dest = to.join(f.strip_prefix(from).unwrap());
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&f, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    Ok(())
}

fn unzip(file: &Path, to: &Path) -> Result<(), String> {
    let mut z = zip::ZipArchive::new(fs::File::open(file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { continue };
        let out = to.join(rel);
        if f.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut w).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Mod archives in the user's Downloads folder from the last 3 days, newest first.
pub fn recent_downloads() -> Vec<(String, PathBuf, u64)> {
    let Some(dir) = std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Downloads")) else { return vec![] };
    let cutoff = SystemTime::now() - Duration::from_secs(3 * 86400);
    let mut out: Vec<(SystemTime, String, PathBuf, u64)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let x = p.extension()?.to_string_lossy().to_lowercase();
            let m = e.metadata().ok()?;
            let t = m.modified().ok()?;
            (["zip", "7z", "pak"].contains(&x.as_str()) && t > cutoff).then(|| (t, e.file_name().to_string_lossy().to_string(), p, m.len()))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().take(12).map(|(_, n, p, s)| (n, p, s)).collect()
}

/// One page of a game's mods on Nexus Mods (public API, no account needed).
pub fn nexus_search(domain: &str, query: &str, sort: &str, offset: u32) -> Result<Value, String> {
    let mut filter = json!({ "gameDomainName": [{ "value": domain, "op": "EQUALS" }] });
    if !query.trim().is_empty() {
        filter["name"] = json!([{ "value": query.trim(), "op": "WILDCARD" }]);
    }
    let sort = match sort {
        "updated" => json!([{ "updatedAt": { "direction": "DESC" } }]),
        "new" => json!([{ "createdAt": { "direction": "DESC" } }]),
        _ => json!([{ "downloads": { "direction": "DESC" } }]),
    };
    let r: Value = ureq::post("https://api.nexusmods.com/v2/graphql")
        .timeout(Duration::from_secs(20))
        .send_json(json!({
            "query": "query($f: ModsFilter, $s: [ModsSort!], $o: Int) { mods(filter: $f, sort: $s, count: 30, offset: $o) { totalCount nodes { modId name summary downloads endorsements author thumbnailUrl version updatedAt } } }",
            "variables": { "f": filter, "s": sort, "o": offset }
        }))
        .map_err(|e| format!("Nexus Mods: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    if let Some(e) = r["errors"].as_array().and_then(|a| a.first()) {
        return Err(format!("Nexus Mods: {}", e["message"].as_str().unwrap_or("error")));
    }
    Ok(r["data"]["mods"].clone())
}
