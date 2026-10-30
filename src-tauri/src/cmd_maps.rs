//! Commands for the Maps page: the web map laid over the window, and the HotSwap map's world,
//! background, player position, teleport and pins.

use super::*;

/// Interactive maps, shown as a second web view inside the main window over the Maps page.
/// The map page is someone else's site, so that view may only call the `map_*` commands
/// (see capabilities/map.json).
pub(crate) const MAPS: [(&str, &str); 1] = [(trainer::SATISFACTORY, "https://satisfactory-calculator.com/en/interactive-map")];

/// Shows the map for `game` at the given spot (CSS pixels of the main window), creating it on first use.
/// Async on purpose: creating a web view from a sync command deadlocks on Windows.
#[tauri::command]
pub(crate) async fn map_show(app: AppHandle, game: String, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let url = MAPS.iter().find(|m| m.0 == game).ok_or("No map for this game")?.1;
    let (pos, size) = (tauri::LogicalPosition::new(x, y), tauri::LogicalSize::new(w.max(1.0), h.max(1.0)));
    if let Some(v) = app.get_webview("map") {
        v.set_position(pos).and_then(|_| v.set_size(size)).and_then(|_| v.show()).map_err(|e| e.to_string())?;
        return v.set_focus().map_err(|e| e.to_string());
    }
    let main = app.get_window("main").ok_or("no main window")?;
    let view = tauri::webview::WebviewBuilder::new("map", tauri::WebviewUrl::External(url.parse().map_err(|e| format!("{e}"))?))
        .initialization_script(include_str!("../../ui/map-overlay.js"));
    main.add_child(view, pos, size).map(|_| ()).map_err(|e| e.to_string())
}

/// Everything the HotSwap map draws, read live from the running game.
#[tauri::command]
pub(crate) async fn map_world(watch: State<'_, watch::Watch>, game: String) -> Result<world::World, String> {
    if game != trainer::SATISFACTORY {
        return Err("No map for this game".into());
    }
    let pid = watch.pid_of(&game).ok_or("Start the game to see your world")?;
    tauri::async_runtime::spawn_blocking(move || ue::objects::Ue::attach(pid).map(|ue| world::scan(&ue)))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub(crate) struct MapBackground {
    overview: PathBuf,
    /// (tile x, tile y, image); 2 x 2 tiles cover the map.
    tiles: Vec<(usize, usize, PathBuf)>,
}

/// The map background, made from the game's own map texture in the user's install (built once,
/// then cached in the app data folder).
#[tauri::command]
pub(crate) async fn map_background(app: AppHandle, watch: State<'_, watch::Watch>, game: String, style: Option<String>) -> Result<MapBackground, String> {
    let install = watch.install_of(&game).ok_or("Game isn't installed")?;
    // Games with more than one map style keep each in its own folder.
    let style = match style.as_deref() { Some("depth") => Some(sn1map::Style::Depth), Some(_) => Some(sn1map::Style::Satellite), None => None };
    let mut dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("map").join(&game);
    if game == sn1::SUBNAUTICA {
        dir = dir.join(if style == Some(sn1map::Style::Depth) { "depth" } else { "satellite" });
    }
    let d = dir.clone();
    let g = game.clone();
    // Satisfactory: the game's own map texture. Subnautica: rendered from its terrain files.
    tauri::async_runtime::spawn_blocking(move || match g.as_str() {
        trainer::SATISFACTORY => mapimage::ensure(&install, &d),
        scrap::SCRAP => scrap::ensure_map(&install, &d),
        sn1::SUBNAUTICA => sn1map::ensure(&install.join(r"Subnautica_Data\StreamingAssets\SNUnmanagedData\Build18"), &d, style.unwrap_or(sn1map::Style::Satellite)),
        _ => Err("No map for this game".into()),
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.asset_protocol_scope().allow_directory(&dir, false);
    Ok(MapBackground {
        overview: dir.join(mapimage::OVERVIEW),
        tiles: [(0, 0), (0, 1), (1, 0), (1, 1)].into_iter().map(|(x, y)| (x, y, dir.join(mapimage::tile_file(x, y)))).collect(),
    })
}

#[tauri::command]
pub(crate) fn map_hide(app: AppHandle) {
    // Closed, not hidden: the web map is a whole website and keeps its memory while hidden.
    if let Some(v) = app.get_webview("map") {
        let _ = v.close();
    }
}

/// The player's x, y, z and yaw while a supported game is running and connected.
#[tauri::command]
pub(crate) fn map_player(trainer: State<trainer::Trainer>) -> Option<[f64; 4]> {
    trainer.position()
}

/// Teleports the player. Without a height, drops them from high above (fall damage is off for a bit).
#[tauri::command]
pub(crate) fn map_teleport(trainer: State<trainer::Trainer>, x: f64, y: f64, z: Option<f64>) -> Result<(), String> {
    if ![x, y, z.unwrap_or(0.0)].iter().all(|v| v.is_finite() && v.abs() < 1e6) {
        return Err("Out of the map".into());
    }
    trainer.position().ok_or("Not connected to the game")?;
    trainer.teleport([x, y, z.unwrap_or(60_000.0)]);
    Ok(())
}

#[tauri::command]
pub(crate) fn map_pins_load(app: AppHandle) -> Result<Option<String>, String> {
    load_data(app, "pins".into())
}

#[tauri::command]
pub(crate) fn map_pins_save(app: AppHandle, data: String) -> Result<(), String> {
    serde_json::from_str::<serde_json::Value>(&data).map_err(|e| e.to_string())?;
    save_data(app, "pins".into(), data)
}
