//! Reads the local Steam install: library folders, installed games, and the
//! artwork Steam already caches for them. Nothing is downloaded or copied.

use crate::appinfo;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Art {
    icon: Option<String>,
    logo: Option<String>,
    hero: Option<String>,
    hero_blur: Option<String>,
    capsule: Option<String>,
    header: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    app_id: u32,
    name: String,
    install_dir: String,
    library: String,
    size_on_disk: u64,
    last_played: u64,
    /// "game" or "software"
    kind: &'static str,
    /// Steam's own label (e.g. "game", "tool", "application"), when appinfo.vdf has one.
    steam_type: Option<String>,
    /// Anti-cheat systems detected, e.g. ["VAC"] or ["Easy Anti-Cheat"]. Empty if none found.
    anticheat: Vec<&'static str>,
    art: Art,
}

impl Game {
    /// Game ID and install folder, for running-game detection. Software is skipped.
    pub fn watch_target(&self) -> Option<(String, PathBuf)> {
        (self.kind == "game" && !self.install_dir.is_empty()).then(|| {
            (self.app_id.to_string(), PathBuf::from(&self.library).join("steamapps").join("common").join(&self.install_dir))
        })
    }
}

#[derive(Serialize)]
pub struct Scan {
    pub root: Option<String>,
    pub libraries: Vec<String>,
    pub games: Vec<Game>,
}

/// Runtimes, redistributables and other entries that show up as "apps" but aren't games.
const NOT_GAMES: &[u32] = &[228980, 250820, 1070560, 1391110, 1628350];
const NOT_GAME_WORDS: &[&str] = &["Redistributable", "Steamworks", "Steam Linux Runtime", "Proton", "Dedicated Server", "Soundtrack"];

/// Backup for when Steam's own type is missing: well-known software people keep in their Steam library.
const KNOWN_SOFTWARE: &[&str] = &[
    "obs studio", "3dmark", "pcmark", "vrmark", "wallpaper engine", "tacview", "crosshair x", "lossless scaling",
    "soundpad", "steamvr", "blender", "vtube studio", "voicemod", "fpsvr", "ovr advanced settings", "ovr toolkit",
    "virtual desktop", "desktop+", "aseprite", "rpg maker", "gamemaker", "source filmmaker", "source sdk", "furmark",
    "superposition benchmark", "shotcut", "krita", "clip studio paint", "substance", "houdini", "moho", "goxel",
    "bandicam", "action!", "streamlabs", "fl studio", "magix", "vegas pro", "ableton", "spacedesk", "duet display",
];

/// Steam types that count as playable. Everything else Steam labels (tool, application, video, ...) is software.
fn kind_of(steam_type: Option<&str>, name: &str) -> &'static str {
    match steam_type {
        Some("game" | "demo" | "mod" | "beta") => "game",
        Some(_) => "software",
        None => {
            let lower = name.to_lowercase();
            if KNOWN_SOFTWARE.iter().any(|s| lower.contains(s)) { "software" } else { "game" }
        }
    }
}

pub fn scan() -> Scan {
    let Some(root) = steam_root() else {
        return Scan { root: None, libraries: vec![], games: vec![] };
    };
    let libraries = library_paths(&root);
    let cache = root.join("appcache").join("librarycache");
    let mut games = Vec::new();

    for lib in &libraries {
        let Ok(entries) = fs::read_dir(lib.join("steamapps")) else { continue };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().to_string();
            if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else { continue };
            let kv = key_values(&text);
            let Some(app_id) = kv.get("appid").and_then(|v| v.parse::<u32>().ok()) else { continue };
            let name = kv.get("name").cloned().unwrap_or_default();
            if name.is_empty() || NOT_GAMES.contains(&app_id) || NOT_GAME_WORDS.iter().any(|w| name.contains(w)) {
                continue;
            }
            games.push(Game {
                app_id,
                name,
                install_dir: kv.get("installdir").cloned().unwrap_or_default(),
                library: lib.to_string_lossy().to_string(),
                size_on_disk: kv.get("SizeOnDisk").and_then(|v| v.parse().ok()).unwrap_or(0),
                last_played: kv.get("LastPlayed").and_then(|v| v.parse().ok()).unwrap_or(0),
                kind: "game",
                steam_type: None,
                anticheat: Vec::new(),
                art: find_art(&cache, app_id),
            });
        }
    }

    let wanted: HashSet<u32> = games.iter().map(|g| g.app_id).collect();
    let mut fields = appinfo::app_fields(&root, &wanted);
    for g in &mut games {
        let f = fields.remove(&g.app_id).unwrap_or_default();
        g.steam_type = f.get("common/type").map(|t| t.to_ascii_lowercase());
        g.kind = kind_of(g.steam_type.as_deref(), &g.name);
        let dir = PathBuf::from(&g.library).join("steamapps").join("common").join(&g.install_dir);
        g.anticheat = detect_anticheat(&f, &dir);
    }

    games.sort_by(|a, b| b.last_played.cmp(&a.last_played).then_with(|| a.name.cmp(&b.name)));
    games.dedup_by_key(|g| g.app_id);

    Scan {
        root: Some(root.to_string_lossy().to_string()),
        libraries: libraries.iter().map(|p| p.to_string_lossy().to_string()).collect(),
        games,
    }
}

/// Steam's data first (VAC category flag, the anti-cheat support link Steam shows on store
/// pages), then well-known anti-cheat files within two folder levels of the install.
fn detect_anticheat(f: &appinfo::Fields, install: &Path) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    let mut add = |name| if !found.contains(&name) { found.push(name) };

    if f.get("common/category/category_8").is_some_and(|v| v == "1") {
        add("VAC");
    }
    if let Some(url) = f.get("extended/anti_cheat_support_url").map(|u| u.to_ascii_lowercase()) {
        add(if url.contains("easy.ac") || url.contains("easyanticheat") {
            "Easy Anti-Cheat"
        } else if url.contains("battleye") {
            "BattlEye"
        } else {
            "Anti-cheat"
        });
    }

    let mut dirs = vec![install.to_path_buf()];
    if let Ok(entries) = fs::read_dir(install) {
        dirs.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
    }
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name == "easyanticheat" || name.starts_with("easyanticheat_") || name == "start_protected_game.exe" {
                add("Easy Anti-Cheat");
            } else if name == "battleye" || name.starts_with("beservice") {
                add("BattlEye");
            } else if name.starts_with("eaanticheat.") {
                add("EA Javelin");
            }
        }
    }
    found
}

pub fn steam_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") {
            if let Ok(path) = key.get_value::<String, _>("SteamPath") {
                let path = PathBuf::from(path.replace('/', "\\"));
                if path.is_dir() {
                    return Some(path);
                }
            }
        }
    }
    [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_dir())
}

fn library_paths(root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![root.to_path_buf()];
    let vdf = root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = fs::read_to_string(vdf) {
        for line in text.lines() {
            if let Some(("path", value)) = pair(line) {
                let path = PathBuf::from(value.replace("\\\\", "\\"));
                let same = libs.iter().any(|l| l.to_string_lossy().eq_ignore_ascii_case(&path.to_string_lossy()));
                if !same && path.is_dir() {
                    libs.push(path);
                }
            }
        }
    }
    libs
}

/// Top-level `"key" "value"` pairs from an .acf/.vdf file (first occurrence wins).
fn key_values(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        if let Some((k, v)) = pair(line) {
            map.entry(k.to_string()).or_insert_with(|| v.to_string());
        }
    }
    map
}

fn pair(line: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = line.trim().split('"').collect();
    // `"key"<ws>"value"` splits into ["", key, ws, value, ""]
    (parts.len() == 5 && parts[2].trim().is_empty()).then(|| (parts[1], parts[3]))
}

/// Steam stores art either as `<appid>/<file>`, `<appid>/<hash>/<file>`, or
/// (older clients) `<appid>_<file>`. The small square icon is a `<sha1>.jpg`.
fn find_art(cache: &Path, app_id: u32) -> Art {
    let mut art = Art::default();
    let dir = cache.join(app_id.to_string());
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(inner) = fs::read_dir(&path) {
                    files.extend(inner.flatten().map(|e| e.path()));
                }
            } else {
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                if stem.len() == 40 && stem.chars().all(|c| c.is_ascii_hexdigit()) {
                    art.icon = Some(path.to_string_lossy().to_string());
                } else {
                    files.push(path);
                }
            }
        }
    }
    for path in files {
        let name = path.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let slot = match name.as_str() {
            "logo.png" => &mut art.logo,
            "library_hero.jpg" => &mut art.hero,
            "library_hero_blur.jpg" => &mut art.hero_blur,
            "library_600x900.jpg" | "library_capsule.jpg" => &mut art.capsule,
            "header.jpg" | "library_header.jpg" => &mut art.header,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(path.to_string_lossy().to_string());
        }
    }
    let flat = |suffix: &str| {
        let p = cache.join(format!("{app_id}_{suffix}"));
        p.is_file().then(|| p.to_string_lossy().to_string())
    };
    art.icon = art.icon.or_else(|| flat("icon.jpg"));
    art.logo = art.logo.or_else(|| flat("logo.png"));
    art.hero = art.hero.or_else(|| flat("library_hero.jpg"));
    art.hero_blur = art.hero_blur.or_else(|| flat("library_hero_blur.jpg"));
    art.capsule = art.capsule.or_else(|| flat("library_600x900.jpg"));
    art.header = art.header.or_else(|| flat("header.jpg"));
    art
}
