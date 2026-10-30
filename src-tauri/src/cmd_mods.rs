//! Commands for the Mods page: ficsit.app (Satisfactory), Nexus Mods files (Subnautica 2 and
//! BepInEx games), Thunderstore and SpaceDock.

use super::*;

/// Mod loader: the game's mod folder must not change while the game runs.
pub(crate) fn mod_install_dir(watch: &watch::Watch, game: &str) -> Result<PathBuf, String> {
    if game != trainer::SATISFACTORY {
        return Err("The mod loader supports Satisfactory only for now".into());
    }
    if watch.running().as_deref() == Some(game) {
        return Err("Close the game first, then change mods".into());
    }
    watch.install_of(game).ok_or_else(|| "Game isn't installed".into())
}

/// Games whose mods are files from Nexus Mods (UE4SS / pak mods): game ID -> (project folder, Nexus domain).
pub(crate) const FILE_MOD_GAMES: [(&str, &str, &str); 1] = [(sn2::SUBNAUTICA2, "Subnautica2", "subnautica2")];

pub(crate) fn file_game(watch: &watch::Watch, game: &str, changing: bool) -> Result<modfiles::Game, String> {
    let (_, project, _) = FILE_MOD_GAMES.iter().find(|g| g.0 == game).ok_or("No mod support for this game")?;
    if changing && watch.running().as_deref() == Some(game) {
        return Err("Close the game first, then change mods".into());
    }
    Ok(modfiles::Game { root: watch.install_of(game).ok_or("Game isn't installed")?, project })
}

#[tauri::command]
pub(crate) fn fmods_list(watch: State<watch::Watch>, game: String) -> Result<Vec<modfiles::FileMod>, String> {
    Ok(file_game(&watch, &game, false)?.list())
}

#[tauri::command]
pub(crate) fn fmods_set_enabled(watch: State<watch::Watch>, game: String, id: String, enabled: bool) -> Result<(), String> {
    file_game(&watch, &game, true)?.set_enabled(&id, enabled)
}

#[tauri::command]
pub(crate) fn fmods_remove(watch: State<watch::Watch>, game: String, id: String) -> Result<(), String> {
    file_game(&watch, &game, true)?.remove(&id)
}

#[tauri::command]
pub(crate) async fn fmods_install(app: AppHandle, watch: State<'_, watch::Watch>, game: String, path: String) -> Result<String, String> {
    let g = file_game(&watch, &game, true)?;
    let scratch = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("modfiles");
    tauri::async_runtime::spawn_blocking(move || g.install(std::path::Path::new(&path), &scratch)).await.map_err(|e| e.to_string())?
}

/// Games whose mods come from Thunderstore: game ID -> Thunderstore community.
/// (Mon Bazou has no Thunderstore community: its BepInEx mods come from Nexus only.)
pub(crate) const THUNDER_GAMES: [(&str, &str); 4] = [(repo::REPO, "repo"), (sn1::SUBNAUTICA, "subnautica"), (sn1::BELOW_ZERO, "subnautica-below-zero"), (extra::MON_BAZOU, "")];

pub(crate) fn thunder_game(watch: &watch::Watch, game: &str, changing: bool) -> Result<(thunderstore::Game, &'static str), String> {
    let (_, community) = THUNDER_GAMES.iter().find(|g| g.0 == game).ok_or("No Thunderstore mods for this game")?;
    if changing && watch.running().as_deref() == Some(game) {
        return Err("Close the game first, then change mods".into());
    }
    Ok((thunderstore::Game { root: watch.install_of(game).ok_or("Game isn't installed")? }, community))
}

// KSP's mods come from SpaceDock (spacedock.rs) through the same commands and shapes.
pub(crate) fn ksp_game(watch: &watch::Watch, game: &str, changing: bool) -> Option<Result<spacedock::Game, String>> {
    (game == ksp::KSP).then(|| {
        if changing && watch.running().as_deref() == Some(game) {
            return Err("Close the game first, then change mods".into());
        }
        Ok(spacedock::Game { root: watch.install_of(game).ok_or("Game isn't installed")? })
    })
}

/// The Mods page was closed: drop the big cached mod lists.
#[tauri::command]
pub(crate) fn mods_release() {
    thunderstore::release();
}

#[tauri::command]
pub(crate) async fn ts_search(game: String, query: String, sort: String, offset: usize) -> Result<serde_json::Value, String> {
    if game == ksp::KSP {
        return tauri::async_runtime::spawn_blocking(move || spacedock::search(&query, &sort, offset)).await.map_err(|e| e.to_string())?;
    }
    let (_, community) = THUNDER_GAMES.iter().find(|g| g.0 == game).ok_or("No Thunderstore mods for this game")?;
    tauri::async_runtime::spawn_blocking(move || thunderstore::search(community, &query, &sort, offset)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) fn ts_list(watch: State<watch::Watch>, game: String) -> Result<serde_json::Value, String> {
    if let Some(g) = ksp_game(&watch, &game, false) {
        return Ok(g?.list());
    }
    serde_json::to_value(thunder_game(&watch, &game, false)?.0.list()).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn ts_install(watch: State<'_, watch::Watch>, game: String, id: String) -> Result<Vec<String>, String> {
    if let Some(g) = ksp_game(&watch, &game, true) {
        let g = g?;
        return tauri::async_runtime::spawn_blocking(move || g.install(&id)).await.map_err(|e| e.to_string())?;
    }
    let (g, community) = thunder_game(&watch, &game, true)?;
    tauri::async_runtime::spawn_blocking(move || g.install(community, &id)).await.map_err(|e| e.to_string())?
}

/// Installs a downloaded mod file (Nexus Mods) into a BepInEx game.
#[tauri::command]
pub(crate) async fn ts_install_file(app: AppHandle, watch: State<'_, watch::Watch>, game: String, path: String) -> Result<String, String> {
    let (g, community) = thunder_game(&watch, &game, true)?;
    let scratch = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || g.install_file(community, std::path::Path::new(&path), &scratch)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) fn ts_set_enabled(watch: State<watch::Watch>, game: String, id: String, enabled: bool) -> Result<(), String> {
    if let Some(g) = ksp_game(&watch, &game, true) {
        return g?.set_enabled(&id, enabled);
    }
    thunder_game(&watch, &game, true)?.0.set_enabled(&id, enabled)
}

#[tauri::command]
pub(crate) fn ts_remove(watch: State<watch::Watch>, game: String, id: String) -> Result<(), String> {
    if let Some(g) = ksp_game(&watch, &game, true) {
        return g?.remove(&id);
    }
    thunder_game(&watch, &game, true)?.0.remove(&id)
}

#[derive(serde::Serialize)]
pub(crate) struct DownloadFile {
    name: String,
    path: PathBuf,
    size: u64,
}

#[tauri::command]
pub(crate) fn fmods_downloads() -> Vec<DownloadFile> {
    modfiles::recent_downloads().into_iter().map(|(name, path, size)| DownloadFile { name, path, size }).collect()
}

/// Lets the user pick a downloaded mod file.
#[tauri::command]
pub(crate) async fn fmods_pick(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("Choose a mod file").add_filter("Mod files", &["zip", "7z", "pak"]).blocking_pick_file()
    })
    .await
    .ok()??;
    picked.into_path().ok().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) async fn nexus_search(game: String, query: String, sort: String, offset: u32) -> Result<serde_json::Value, String> {
    let (_, _, domain) = FILE_MOD_GAMES.iter().find(|g| g.0 == game).ok_or("No mod support for this game")?;
    tauri::async_runtime::spawn_blocking(move || modfiles::nexus_search(domain, &query, &sort, offset)).await.map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub(crate) struct ModsState {
    installed: Vec<modloader::Installed>,
    build: Option<u64>,
}

#[tauri::command]
pub(crate) fn mods_state(watch: State<watch::Watch>, game: String) -> Result<ModsState, String> {
    let install = watch.install_of(&game).ok_or("Game isn't installed")?;
    Ok(ModsState { installed: modloader::installed(&install), build: modloader::game_build(&install) })
}

#[tauri::command]
pub(crate) async fn mods_search(query: String, sort: String, offset: u32) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || modloader::search(&query, &sort, offset)).await.map_err(|e| e.to_string())?
}

/// Installs or updates a mod with its dependencies; progress arrives as `mod-progress` events.
#[tauri::command]
pub(crate) async fn mods_install(app: AppHandle, watch: State<'_, watch::Watch>, game: String, reference: String) -> Result<Vec<String>, String> {
    let install = mod_install_dir(&watch, &game)?;
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("mods");
    let a = app.clone();
    tauri::async_runtime::spawn_blocking(move || modloader::install(&install, &reference, &cache, |p| { let _ = a.emit("mod-progress", p); }))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) fn mods_uninstall(watch: State<watch::Watch>, game: String, reference: String) -> Result<(), String> {
    modloader::uninstall(&mod_install_dir(&watch, &game)?, &reference)
}

#[tauri::command]
pub(crate) fn mods_set_enabled(watch: State<watch::Watch>, game: String, reference: String, enabled: bool) -> Result<(), String> {
    modloader::set_enabled(&mod_install_dir(&watch, &game)?, &reference, enabled)
}

#[tauri::command]
pub(crate) async fn mods_updates(watch: State<'_, watch::Watch>, game: String) -> Result<HashMap<String, String>, String> {
    let install = watch.install_of(&game).ok_or("Game isn't installed")?;
    tauri::async_runtime::spawn_blocking(move || modloader::updates(&install)).await.map_err(|e| e.to_string())?
}
