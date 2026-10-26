//! Installed Epic Games Store titles, read from the launcher's local manifest files
//! (`%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item`, plain JSON).

use crate::steam::{detect_anticheat, kind_of, Art, Game, Launch};
use std::{fs, path::PathBuf, time::UNIX_EPOCH};

pub fn scan() -> Vec<Game> {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
    let dir = PathBuf::from(base).join(r"Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(entries) = fs::read_dir(dir) else { return vec![] };
    let mut games = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Ok(m) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let s = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let (name, app, install) = (s("DisplayName"), s("AppName"), PathBuf::from(s("InstallLocation")));
        if name.is_empty() || app.is_empty() || m.get("bIsIncompleteInstall").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        let cats: Vec<String> = m.get("AppCategories").and_then(|v| v.as_array()).into_iter().flatten()
            .filter_map(|c| c.as_str().map(str::to_ascii_lowercase)).collect();
        // DLC and add-ons get their own manifests; skip them.
        if cats.iter().any(|c| c == "addons" || c == "addons/launchable") {
            continue;
        }
        let kind = if cats.iter().any(|c| c == "applications") && !cats.iter().any(|c| c == "games") {
            "software"
        } else {
            kind_of(None, &name)
        };
        let url = format!(
            "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=launch&silent=true",
            s("CatalogNamespace"), s("CatalogItemId"), app
        );
        let last_played = fs::metadata(&path).and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        games.push(Game {
            id: format!("epic:{app}"),
            source: "epic",
            app_id: 0,
            name,
            size_on_disk: m.get("InstallSize").and_then(|v| v.as_u64()).unwrap_or(0),
            last_played,
            playtime: 0,
            kind,
            steam_type: None,
            anticheat: detect_anticheat(&Default::default(), &install),
            art: Art::default(),
            install,
            launch: Launch::Url(url),
        });
    }
    games
}
