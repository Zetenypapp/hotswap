//! Installed GOG games, from the registry keys GOG installers write
//! (`HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>`).

use crate::steam::{detect_anticheat, kind_of, Art, Game, Launch};
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
pub fn scan() -> Vec<Game> {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    let mut games = Vec::new();
    let Ok(root) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\GOG.com\Games") else {
        return games;
    };
    for id in root.enum_keys().flatten() {
        let Ok(key) = root.open_subkey(&id) else { continue };
        let get = |k: &str| key.get_value::<String, _>(k).unwrap_or_default();
        let (name, path, exe) = (get("gameName"), PathBuf::from(get("path")), get("exe"));
        // DLCs register with a parent game in "dependsOn".
        if name.is_empty() || !get("dependsOn").is_empty() || !path.is_dir() {
            continue;
        }
        let exe = PathBuf::from(&exe);
        let exe = if exe.is_absolute() { exe } else { path.join(exe) };
        let dir = Some(get("workingDir")).filter(|d| !d.is_empty()).map(PathBuf::from).unwrap_or_else(|| path.clone());
        games.push(Game {
            id: format!("gog:{id}"),
            source: "gog",
            app_id: 0,
            kind: kind_of(None, &name),
            name,
            size_on_disk: 0,
            last_played: 0,
            playtime: 0,
            steam_type: None,
            anticheat: detect_anticheat(&Default::default(), &path),
            art: Art::default(),
            launch: if exe.is_file() { Launch::Exe { path: exe, dir } } else { Launch::None },
            install: path,
        });
    }
    games
}

#[cfg(not(windows))]
pub fn scan() -> Vec<Game> {
    Vec::new()
}
