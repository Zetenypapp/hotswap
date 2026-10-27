#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appinfo;
mod epic;
mod gog;
mod home;
mod iostore;
mod mapimage;
mod modfiles;
mod modloader;
mod ksp;
mod scrap;
mod repo;
mod mono;
mod sn1;
mod sn1map;
mod sn2;
mod steam;
mod trainer;
mod ue;
mod vdf;
mod watch;
mod world;

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Registered system-wide hotkeys: shortcut ID -> the combo string the UI asked for.
#[derive(Default)]
struct Hotkeys(Mutex<HashMap<u32, String>>);

/// How to start each scanned game, by game ID.
#[derive(Default)]
struct Launches(Mutex<HashMap<String, steam::Launch>>);

/// When true, closing the window hides it to the tray instead of quitting.
struct CloseToTray(AtomicBool);

/// The main window's normal size and position, remembered between runs (logical pixels).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy)]
struct Geometry {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    max: bool,
}

/// Mini mode: a small always-on-top window. `back` is where the window was before.
#[derive(Default)]
struct Mini {
    on: AtomicBool,
    back: Mutex<Option<Geometry>>,
}

const MIN_SIZE: (f64, f64) = (1000.0, 600.0);

fn geometry_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("window.json"))
}

fn geometry_of(win: &tauri::WebviewWindow) -> Option<Geometry> {
    let s = win.scale_factor().ok()?;
    let p = win.outer_position().ok()?.to_logical::<f64>(s);
    let z = win.inner_size().ok()?.to_logical::<f64>(s);
    Some(Geometry { x: p.x, y: p.y, w: z.width, h: z.height, max: win.is_maximized().unwrap_or(false) })
}

/// Saves the main window's geometry after a move or resize (not while minimized or in mini mode;
/// while maximized only the flag changes, so un-maximizing next run goes back to the old size).
fn remember_geometry(win: &tauri::Window) {
    let app = win.app_handle();
    if app.state::<Mini>().on.load(Ordering::Relaxed) || win.is_minimized().unwrap_or(true) || !win.is_visible().unwrap_or(false) {
        return;
    }
    let Some(webview) = app.get_webview_window("main") else { return };
    let Some(mut g) = geometry_of(&webview) else { return };
    let Some(file) = geometry_file(app) else { return };
    if g.max {
        let Some(old) = fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str::<Geometry>(&s).ok()) else { return };
        g = Geometry { max: true, ..old };
    }
    if let Ok(json) = serde_json::to_string(&g) {
        let _ = fs::write(file, json);
    }
}

/// Turns mini mode on or off.
#[tauri::command]
fn set_mini(app: AppHandle, mini: State<Mini>, on: bool) -> Result<(), String> {
    let win = app.get_webview_window("main").ok_or("no window")?;
    let e = |e: tauri::Error| e.to_string();
    if on {
        if !mini.on.swap(true, Ordering::Relaxed) {
            *mini.back.lock().unwrap() = geometry_of(&win);
        }
        let back = *mini.back.lock().unwrap();
        let _ = win.unmaximize();
        win.set_min_size(Some(tauri::LogicalSize::new(300.0, 260.0))).map_err(e)?;
        win.set_size(tauri::LogicalSize::new(340.0, 520.0)).map_err(e)?;
        // Top-right of where the big window was.
        if let Some(b) = back {
            win.set_position(tauri::LogicalPosition::new(b.x + b.w - 340.0 - 16.0, b.y + 16.0)).map_err(e)?;
        }
        win.set_always_on_top(true).map_err(e)?;
    } else {
        win.set_always_on_top(false).map_err(e)?;
        win.set_min_size(Some(tauri::LogicalSize::new(MIN_SIZE.0, MIN_SIZE.1))).map_err(e)?;
        let back = mini.back.lock().unwrap().take();
        match back {
            Some(b) => {
                win.set_size(tauri::LogicalSize::new(b.w.max(MIN_SIZE.0), b.h.max(MIN_SIZE.1))).map_err(e)?;
                win.set_position(tauri::LogicalPosition::new(b.x, b.y)).map_err(e)?;
                if b.max {
                    let _ = win.maximize();
                }
            }
            None => {
                win.set_size(tauri::LogicalSize::new(1440.0, 880.0)).map_err(e)?;
                let _ = win.center();
            }
        }
        mini.on.store(false, Ordering::Relaxed);
    }
    Ok(())
}

/// Files the UI may read and write in the app data folder.
fn data_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let file = match name {
        "state" => "state.json",
        "home" => "home_cache.json",
        "pins" => "map_pins.json",
        _ => return Err(format!("Unknown data file: {name}")),
    };
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(file))
}

/// Returns a saved data file as raw JSON, or None if it doesn't exist yet.
#[tauri::command]
fn load_data(app: AppHandle, name: String) -> Result<Option<String>, String> {
    Ok(fs::read_to_string(data_file(&app, &name)?).ok())
}

/// Writes through a temp file so a crash mid-write can't corrupt the data.
#[tauri::command]
fn save_data(app: AppHandle, name: String, data: String) -> Result<(), String> {
    let path = data_file(&app, &name)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Finds installed games (Steam, Epic, GOG), lets the webview load Steam's cached
/// artwork, points the game watcher at the install folders, and remembers how to launch each.
#[tauri::command]
fn scan_library(app: AppHandle, watch: State<watch::Watch>, launches: State<Launches>) -> steam::Scan {
    let scan = steam::scan();
    if let Some(root) = &scan.root {
        let cache = PathBuf::from(root).join("appcache").join("librarycache");
        let _ = app.asset_protocol_scope().allow_directory(&cache, true);
    }
    watch.set_targets(scan.games.iter().filter_map(|g| g.watch_target()).collect());
    *launches.0.lock().unwrap() = scan.games.iter().map(|g| (g.id.clone(), g.launch.clone())).collect();
    scan
}

/// The game currently running, if any (the UI also gets `game-running` events).
#[tauri::command]
fn running_game(watch: State<watch::Watch>) -> Option<String> {
    watch.running()
}

/// Starts a game through its store's launcher (or its executable for GOG).
#[tauri::command]
fn launch_game(launches: State<Launches>, id: String) -> Result<(), String> {
    let launch = launches.0.lock().unwrap().get(&id).cloned().unwrap_or_default();
    let result = match launch {
        steam::Launch::Url(url) => std::process::Command::new("explorer.exe").arg(url).spawn(),
        steam::Launch::Exe { path, dir } => std::process::Command::new(path).current_dir(dir).spawn(),
        steam::Launch::None => return Err("Hotswap doesn't know how to start this game".into()),
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Replaces all system-wide hotkeys with `keys` (e.g. "F1", "Shift+F9", "Ctrl+KeyK").
/// Returns the combos that couldn't be registered, usually because another app holds them.
#[tauri::command]
fn set_hotkeys(app: AppHandle, hotkeys: State<Hotkeys>, keys: Vec<String>) -> Vec<String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut map = hotkeys.0.lock().unwrap();
    map.clear();
    let mut failed = Vec::new();
    for key in keys {
        match key.parse::<Shortcut>() {
            Ok(sc) if gs.register(sc).is_ok() => {
                map.insert(sc.id(), key);
            }
            _ => failed.push(key),
        }
    }
    failed
}

/// Fetches news, deals and store lists for Home. Runs off the UI thread.
#[tauri::command]
async fn fetch_home(app_ids: Vec<u32>) -> Result<home::Home, String> {
    tauri::async_runtime::spawn_blocking(move || home::fetch(&app_ids))
        .await
        .map_err(|e| e.to_string())
}

/// The mod values the running game should have. `values` are numbers: toggles 0/1, sliders their value.
#[tauri::command]
fn trainer_set(trainer: State<trainer::Trainer>, game: Option<String>, paused: bool, values: HashMap<String, f64>) {
    trainer.set(game, paused, values);
}

/// One-shot mods like "Add 1,000 Coupons".
#[tauri::command]
fn trainer_action(trainer: State<trainer::Trainer>, id: String) {
    trainer.action(id);
}

/// Force-closes a running game. The game doesn't get a chance to save.
#[tauri::command]
fn close_game(watch: State<watch::Watch>, id: String) -> usize {
    watch.kill(&id)
}

/// The app was called Tinker; carry its saved settings over to the new data folder once.
fn migrate_old_data(app: &tauri::App) {
    let Ok(dir) = app.path().app_data_dir() else { return };
    let old = dir.with_file_name("app.tinker.desktop");
    if dir.join("state.json").exists() || !old.is_dir() {
        return;
    }
    let _ = fs::create_dir_all(&dir);
    for name in ["state.json", "home_cache.json"] {
        let _ = fs::copy(old.join(name), dir.join(name));
    }
}

/// Interactive maps, shown as a second web view inside the main window over the Maps page.
/// The map page is someone else's site, so that view may only call the `map_*` commands
/// (see capabilities/map.json).
const MAPS: [(&str, &str); 1] = [(trainer::SATISFACTORY, "https://satisfactory-calculator.com/en/interactive-map")];

/// Shows the map for `game` at the given spot (CSS pixels of the main window), creating it on first use.
/// Async on purpose: creating a web view from a sync command deadlocks on Windows.
#[tauri::command]
async fn map_show(app: AppHandle, game: String, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
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

/// Everything the Hotswap map draws, read live from the running game.
#[tauri::command]
async fn map_world(watch: State<'_, watch::Watch>, game: String) -> Result<world::World, String> {
    if game != trainer::SATISFACTORY {
        return Err("No map for this game".into());
    }
    let pid = watch.pid_of(&game).ok_or("Start the game to see your world")?;
    tauri::async_runtime::spawn_blocking(move || ue::objects::Ue::attach(pid).map(|ue| world::scan(&ue)))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
struct MapBackground {
    overview: PathBuf,
    /// (tile x, tile y, image); 2 x 2 tiles cover the map.
    tiles: Vec<(usize, usize, PathBuf)>,
}

/// The map background, made from the game's own map texture in the user's install (built once,
/// then cached in the app data folder).
#[tauri::command]
async fn map_background(app: AppHandle, watch: State<'_, watch::Watch>, game: String, style: Option<String>) -> Result<MapBackground, String> {
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
fn map_hide(app: AppHandle) {
    if let Some(v) = app.get_webview("map") {
        let _ = v.hide();
    }
}

/// The player's x, y, z and yaw while a supported game is running and connected.
#[tauri::command]
fn map_player(trainer: State<trainer::Trainer>) -> Option<[f64; 4]> {
    trainer.position()
}

/// Teleports the player. Without a height, drops them from high above (fall damage is off for a bit).
#[tauri::command]
fn map_teleport(trainer: State<trainer::Trainer>, x: f64, y: f64, z: Option<f64>) -> Result<(), String> {
    if ![x, y, z.unwrap_or(0.0)].iter().all(|v| v.is_finite() && v.abs() < 1e6) {
        return Err("Out of the map".into());
    }
    trainer.position().ok_or("Not connected to the game")?;
    trainer.teleport([x, y, z.unwrap_or(60_000.0)]);
    Ok(())
}

#[tauri::command]
fn map_pins_load(app: AppHandle) -> Result<Option<String>, String> {
    load_data(app, "pins".into())
}

#[tauri::command]
fn map_pins_save(app: AppHandle, data: String) -> Result<(), String> {
    serde_json::from_str::<serde_json::Value>(&data).map_err(|e| e.to_string())?;
    save_data(app, "pins".into(), data)
}

/// Mod loader: the game's mod folder must not change while the game runs.
fn mod_install_dir(watch: &watch::Watch, game: &str) -> Result<PathBuf, String> {
    if game != trainer::SATISFACTORY {
        return Err("The mod loader supports Satisfactory only for now".into());
    }
    if watch.running().as_deref() == Some(game) {
        return Err("Close the game first, then change mods".into());
    }
    watch.install_of(game).ok_or_else(|| "Game isn't installed".into())
}

/// Games whose mods are files from Nexus Mods (UE4SS / pak mods): game ID -> (project folder, Nexus domain).
const FILE_MOD_GAMES: [(&str, &str, &str); 1] = [(sn2::SUBNAUTICA2, "Subnautica2", "subnautica2")];

fn file_game(watch: &watch::Watch, game: &str, changing: bool) -> Result<modfiles::Game, String> {
    let (_, project, _) = FILE_MOD_GAMES.iter().find(|g| g.0 == game).ok_or("No mod support for this game")?;
    if changing && watch.running().as_deref() == Some(game) {
        return Err("Close the game first, then change mods".into());
    }
    Ok(modfiles::Game { root: watch.install_of(game).ok_or("Game isn't installed")?, project })
}

#[tauri::command]
fn fmods_list(watch: State<watch::Watch>, game: String) -> Result<Vec<modfiles::FileMod>, String> {
    Ok(file_game(&watch, &game, false)?.list())
}

#[tauri::command]
fn fmods_set_enabled(watch: State<watch::Watch>, game: String, id: String, enabled: bool) -> Result<(), String> {
    file_game(&watch, &game, true)?.set_enabled(&id, enabled)
}

#[tauri::command]
fn fmods_remove(watch: State<watch::Watch>, game: String, id: String) -> Result<(), String> {
    file_game(&watch, &game, true)?.remove(&id)
}

#[tauri::command]
async fn fmods_install(app: AppHandle, watch: State<'_, watch::Watch>, game: String, path: String) -> Result<String, String> {
    let g = file_game(&watch, &game, true)?;
    let scratch = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("modfiles");
    tauri::async_runtime::spawn_blocking(move || g.install(std::path::Path::new(&path), &scratch)).await.map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
struct DownloadFile {
    name: String,
    path: PathBuf,
    size: u64,
}

#[tauri::command]
fn fmods_downloads() -> Vec<DownloadFile> {
    modfiles::recent_downloads().into_iter().map(|(name, path, size)| DownloadFile { name, path, size }).collect()
}

/// Lets the user pick a downloaded mod file.
#[tauri::command]
async fn fmods_pick(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("Choose a mod file").add_filter("Mod files", &["zip", "7z", "pak"]).blocking_pick_file()
    })
    .await
    .ok()??;
    picked.into_path().ok().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
async fn nexus_search(game: String, query: String, sort: String, offset: u32) -> Result<serde_json::Value, String> {
    let (_, _, domain) = FILE_MOD_GAMES.iter().find(|g| g.0 == game).ok_or("No mod support for this game")?;
    tauri::async_runtime::spawn_blocking(move || modfiles::nexus_search(domain, &query, &sort, offset)).await.map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
struct ModsState {
    installed: Vec<modloader::Installed>,
    build: Option<u64>,
}

#[tauri::command]
fn mods_state(watch: State<watch::Watch>, game: String) -> Result<ModsState, String> {
    let install = watch.install_of(&game).ok_or("Game isn't installed")?;
    Ok(ModsState { installed: modloader::installed(&install), build: modloader::game_build(&install) })
}

#[tauri::command]
async fn mods_search(query: String, sort: String, offset: u32) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || modloader::search(&query, &sort, offset)).await.map_err(|e| e.to_string())?
}

/// Installs or updates a mod with its dependencies; progress arrives as `mod-progress` events.
#[tauri::command]
async fn mods_install(app: AppHandle, watch: State<'_, watch::Watch>, game: String, reference: String) -> Result<Vec<String>, String> {
    let install = mod_install_dir(&watch, &game)?;
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("mods");
    let a = app.clone();
    tauri::async_runtime::spawn_blocking(move || modloader::install(&install, &reference, &cache, |p| { let _ = a.emit("mod-progress", p); }))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn mods_uninstall(watch: State<watch::Watch>, game: String, reference: String) -> Result<(), String> {
    modloader::uninstall(&mod_install_dir(&watch, &game)?, &reference)
}

#[tauri::command]
fn mods_set_enabled(watch: State<watch::Watch>, game: String, reference: String, enabled: bool) -> Result<(), String> {
    modloader::set_enabled(&mod_install_dir(&watch, &game)?, &reference, enabled)
}

#[tauri::command]
async fn mods_updates(watch: State<'_, watch::Watch>, game: String) -> Result<HashMap<String, String>, String> {
    let install = watch.install_of(&game).ok_or("Game isn't installed")?;
    tauri::async_runtime::spawn_blocking(move || modloader::updates(&install)).await.map_err(|e| e.to_string())?
}

/// Where a window of size w x h goes on the main monitor: "top" (centered), "top-left",
/// "top-right", "bottom-left" or "bottom-right" (logical pixels; bottom stays clear of the taskbar).
fn corner_pos(app: &AppHandle, corner: &str, w: f64, h: f64, margin: f64) -> Result<(f64, f64), String> {
    let m = app.primary_monitor().ok().flatten().ok_or("no monitor")?;
    let s = m.scale_factor();
    let (mx, my) = (m.position().x as f64 / s, m.position().y as f64 / s);
    let (mw, mh) = (m.size().width as f64 / s, m.size().height as f64 / s);
    let x = if corner.ends_with("left") {
        mx + margin
    } else if corner.ends_with("right") {
        mx + mw - w - margin
    } else {
        mx + mw / 2.0 - w / 2.0
    };
    let y = if corner.starts_with("bottom") { my + mh - h - margin - 48.0 } else { my + margin };
    Ok((x, y))
}

/// Shows a mod change (or the hotkey cheat sheet) in the overlay over the game. Created on first
/// use, then kept: a transparent, click-through, always-on-top window in the chosen corner.
#[tauri::command]
async fn overlay_notify(app: AppHandle, note: serde_json::Value) -> Result<(), String> {
    let corner = note.get("corner").and_then(|c| c.as_str()).unwrap_or("top").to_string();
    // The cheat sheet stays while its key is held (up to 10 s); a closed sheet needs no window.
    let sheet_open = note.get("kind").and_then(|k| k.as_str()) == Some("sheet") && note.get("show").and_then(|v| v.as_bool()) == Some(true);
    let (w, h) = (460.0, 600.0);
    let (x, y) = corner_pos(&app, &corner, w, h, 0.0)?;
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.set_position(tauri::LogicalPosition::new(x, y));
    } else {
        let win = tauri::WebviewWindowBuilder::new(&app, "overlay", tauri::WebviewUrl::App("overlay.html".into()))
            .title("Hotswap overlay")
            .inner_size(w, h)
            .position(x, y)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .resizable(false)
            .build()
            .map_err(|e| e.to_string())?;
        let _ = win.set_ignore_cursor_events(true);
        // Give the page a moment to start listening before the first note.
        tokio_sleep(400).await;
    }

    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.show();
    }
    app.emit_to("overlay", "overlay", note).map_err(|e| e.to_string())?;
    // Hidden again once the notes are gone: a see-through window kept over a game can make
    // Windows drop the game's fast fullscreen path, which shows up as stutter.
    let id = OVERLAY_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    let hold = if sheet_open { 10_500 } else { 3_000 };
    tauri::async_runtime::spawn(async move {
        tokio_sleep(hold).await;
        if OVERLAY_GEN.load(Ordering::Relaxed) == id {
            if let Some(win) = app.get_webview_window("overlay") {
                let _ = win.hide();
            }
        }
    });
    Ok(())
}

/// Bumped on every overlay note; a pending hide only runs if no newer note came.
static OVERLAY_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

async fn tokio_sleep(ms: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_millis(ms))).await;
}

/// Shows, moves or closes the radar: a small transparent, click-through window in a corner.
#[tauri::command]
async fn radar_show(app: AppHandle, show: bool, corner: Option<String>) -> Result<(), String> {
    let existing = app.get_webview_window("radar");
    if !show {
        if let Some(w) = existing {
            let _ = w.close();
        }
        return Ok(());
    }
    let size = 260.0;
    let (x, y) = corner_pos(&app, corner.as_deref().unwrap_or("top-right"), size, size, 24.0)?;
    if let Some(w) = existing {
        let _ = w.set_position(tauri::LogicalPosition::new(x, y));
        return Ok(());
    }
    let win = tauri::WebviewWindowBuilder::new(&app, "radar", tauri::WebviewUrl::App("radar.html".into()))
        .title("Hotswap radar")
        .inner_size(size, size)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .resizable(false)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_ignore_cursor_events(true);
    Ok(())
}

#[tauri::command]
fn overlay_close(app: AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.close();
    }
}

#[derive(serde::Serialize)]
struct UpdateInfo {
    version: String,
    notes: String,
}

/// A newer Hotswap release on GitHub, if there is one (signed; checked against the built-in key).
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo { version: u.version.clone(), notes: u.body.clone().unwrap_or_default() }))
}

/// Downloads and installs the newest release, then restarts into it.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?.ok_or("Already up to date")?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    app.restart();
}

#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
fn set_close_to_tray(flag: State<CloseToTray>, enabled: bool) {
    flag.0.store(enabled, Ordering::Relaxed);
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let al = app.autolaunch();
    if enabled { al.enable() } else { al.disable() }.map_err(|e| e.to_string())
}

fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Fits the window to the monitor (at high display scaling 1440x880 can be taller
/// than the screen). The window starts hidden to avoid a visible jump.
fn fit_window(app: &tauri::App, show: bool) {
    let Some(win) = app.get_webview_window("main") else { return };
    // The size and place it had last time, if that's still on a connected monitor.
    let saved = geometry_file(app.handle()).and_then(|f| fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str::<Geometry>(&s).ok());
    let on_screen = |g: &Geometry| {
        win.available_monitors().unwrap_or_default().iter().any(|m| {
            let s = m.scale_factor();
            let (mx, my) = (m.position().x as f64 / s, m.position().y as f64 / s);
            let (mw, mh) = (m.size().width as f64 / s, m.size().height as f64 / s);
            g.x + 60.0 >= mx && g.x + 60.0 < mx + mw && g.y + 10.0 >= my && g.y + 10.0 < my + mh
        })
    };
    if let Some(g) = saved.filter(|g| g.w >= MIN_SIZE.0 && g.h >= MIN_SIZE.1 && on_screen(g)) {
        let _ = win.set_size(tauri::LogicalSize::new(g.w, g.h));
        let _ = win.set_position(tauri::LogicalPosition::new(g.x, g.y));
        if g.max {
            let _ = win.maximize();
        }
    } else if let Ok(Some(monitor)) = win.current_monitor() {
        let screen = monitor.size().to_logical::<f64>(monitor.scale_factor());
        let size = tauri::LogicalSize::new((screen.width * 0.9).min(1440.0), (screen.height * 0.88).min(880.0));
        let _ = win.set_size(size);
        let _ = win.center();
    }
    if show {
        let _ = win.show();
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Hotswap", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Hotswap", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Hotswap")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn main() {
    // Dev aid: `tinker --scan` prints what the library scanner finds and exits.
    if std::env::args().any(|a| a == "--scan") {
        println!("{}", serde_json::to_string_pretty(&steam::scan()).unwrap_or_default());
        return;
    }
    // Dev aid: `tinker --appinfo <appid>` prints Steam's appinfo.vdf fields for one app.
    if let Some(id) = std::env::args().skip_while(|a| a != "--appinfo").nth(1).and_then(|s| s.parse::<u32>().ok()) {
        if let Some(root) = steam::steam_root() {
            let mut fields: Vec<_> = appinfo::app_fields(&root, &[id].into()).remove(&id).unwrap_or_default().into_iter().collect();
            fields.sort();
            for (k, v) in fields {
                println!("{k} = {v}");
            }
        }
        return;
    }
    // Dev aid: `tinker --home` fetches the live Home data once and prints it.
    if std::env::args().any(|a| a == "--home") {
        let ids: Vec<u32> = steam::scan().games.iter().filter(|g| g.kind == "game").map(|g| g.app_id).filter(|&id| id != 0).take(6).collect();
        println!("{}", serde_json::to_string_pretty(&home::fetch(&ids)).unwrap_or_default());
        return;
    }
    // Dev aid: `hotswap --pdb <file.pdb> <name>` prints a symbol's RVA.
    if let Some(i) = std::env::args().position(|a| a == "--pdb") {
        let a: Vec<String> = std::env::args().collect();
        let t = std::time::Instant::now();
        match ue::symbols::find_rva(std::path::Path::new(&a[i + 1]), &a[i + 2]) {
            Ok(rva) => println!("{} = 0x{rva:X} ({:?})", a[i + 2], t.elapsed()),
            Err(e) => println!("error: {e} ({:?})", t.elapsed()),
        }
        return;
    }
    // Dev aid: `hotswap --map-image <out dir>` builds the map background from the game files.
    if let Some(out) = std::env::args().skip_while(|a| a != "--map-image").nth(1) {
        let t = std::time::Instant::now();
        let install = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\Satisfactory");
        println!("{:?} in {:?}", mapimage::ensure(install, std::path::Path::new(&out)), t.elapsed());
        return;
    }
    // Dev aid: `hotswap --mods-plan <ModReference>` shows what installing a mod would download.
    if let Some(r) = std::env::args().skip_while(|a| a != "--mods-plan").nth(1) {
        let install = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\Satisfactory");
        println!("build {:?}", modloader::game_build(install));
        println!("{}", serde_json::to_string(&modloader::installed(install)).unwrap());
        println!("{:?}", modloader::dry_run(install, &r));
        println!("{:?}", modloader::search("smart", "downloads", 0).map(|v| v["count"].clone()));
        return;
    }
    // Dev aid: `hotswap --scan-attach <exe name part> [Class [filter]]` attaches without symbols and
    // lists a class's properties (or counts objects).
    if let Some(m) = std::env::args().skip_while(|a| a != "--scan-attach").nth(1) {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains(&m)).map(|p| p.pid().as_u32());
        let t = std::time::Instant::now();
        if let Some(pp) = pid.and_then(|p| ue::mem::Proc::open(p).ok()) {
            let (base, _) = pp.module(&m).unwrap_or_default();
            for s in ue::scan::sections(&pp, base) { println!("section {} 0x{:X} size 0x{:X}", s.0, s.1, s.2); }
            let (o, n) = ue::scan::find_tables(&pp, base);
            println!("object candidates {} name candidates {}", o.len(), n.len());
            for &c in o.iter().take(5) { println!("  obj cand 0x{c:X} num {:?}", pp.read::<i32>(c + 0x24)); }
            // Look for the "None" entry: a block whose first bytes are a 4-char header then "None".
            let mut shown = 0;
            for &c in n.iter() {
                let Some(b0) = pp.ptr(c + 0x10) else { continue };
                let mut h = [0u8; 16];
                pp.read_into(b0, &mut h);
                if h[2..6] == *b"None" || h[4..8] == *b"None" || h[..8].windows(4).any(|w| w == b"None") {
                    println!("  name cand 0x{c:X} block0 {:02X?}", h); shown += 1;
                    if shown > 5 { break; }
                }
            }
        }
        let u = match pid.map(|p| ue::objects::Ue::attach_scan(p, &m)) { Some(Ok(u)) => u, Some(Err(e)) => return println!("attach failed: {e}"), None => return println!("not running") };
        println!("attached in {:?}: {} objects", t.elapsed(), u.count());
        let args: Vec<String> = std::env::args().skip_while(|a| a != "--scan-attach").skip(2).collect();
        if let Some(cls) = args.first() {
            let filter = args.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
            if cls == "*" {
                // Class names matching the filter, with how many live objects each has.
                let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
                for o in u.all_objects() {
                    if let Some(c) = u.class_of(o).and_then(|c| u.name_of(c)) {
                        if c.to_lowercase().contains(&filter) { *counts.entry(c).or_default() += 1; }
                    }
                }
                for (k, n) in counts { println!("{n:6} {k}"); }
            } else if let Some(c) = u.find_class(cls) {
                for p in u.props(c).into_iter().filter(|p| p.name.to_lowercase().contains(&filter)) {
                    println!("{}.{} : {} @0x{:X}", p.owner, p.name, p.kind, p.offset);
                }
            } else {
                println!("class {cls} not found");
            }
        }
        return;
    }
    // Dev aid: `hotswap --sn2-peek` shows the Subnautica 2 player's attribute sets (base / current).
    if std::env::args().any(|a| a == "--sn2-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("Subnautica2-Win64")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach_scan(p, "Subnautica2-Win64").ok()) else { return println!("not attached") };
        let all = u.all_objects();
        println!("count {} live {}", u.count(), all.len());
        for &o in &all {
            if u.class_of(o).and_then(|c| u.name_of(c)).is_some_and(|n| n == "BP_SN2PlayerCharacter_C" || n == "BP_SN2PlayerController_C") {
                println!("  {} 0x{o:X} outer {:?}", u.name_of(o).unwrap_or_default(), u.outer_of(o).and_then(|x| u.name_of(x)));
                if let Some(c) = u.class_of(o) {
                    for f in ["Pawn", "AcknowledgedPawn", "Character", "PlayerState"] {
                        let v = u.prop(c, f).and_then(|pr| u.p.ptr(o + pr.offset as u64));
                        println!("    {f} = {v:X?} {:?}", v.and_then(|x| u.class_of(x)).and_then(|c| u.name_of(c)));
                    }
                }
            }
        }
        // The local player: what a live player controller possesses, plus its player state.
        let mut players = Vec::new();
        for c in u.find_by_classes(|n| n == "BP_SN2PlayerController_C") {
            let cls = u.class_of(c).unwrap();
            for f in ["Pawn", "PlayerState"] {
                if let Some(x) = u.prop(cls, f).and_then(|pr| u.p.ptr(c + pr.offset as u64)) { players.push(x); }
            }
        }
        for &p in &players {
            let cls = u.class_of(p).unwrap();
            let ctrl = u.prop(cls, "Controller").and_then(|o| u.p.ptr(p + o.offset as u64));
            println!("player 0x{p:X} {} {} controller={ctrl:X?}", u.name_of(cls).unwrap_or_default(), u.name_of(p).unwrap_or_default());
        }
        if let Some(&pawn) = players.first() {
            let pc = u.class_of(pawn).unwrap();
            if let Some(m) = u.prop(pc, "CharacterMovement").and_then(|o| u.p.ptr(pawn + o.offset as u64)) {
                let mc = u.class_of(m).unwrap();
                println!("movement 0x{m:X} {}", u.name_of(mc).unwrap_or_default());
                for f in ["MaxWalkSpeed", "MaxSwimSpeed", "MaxFlySpeed", "MaxCustomMovementSpeed", "MaxAcceleration", "ForwardAndStrafeAddWalkSpeed", "MovementMode", "CustomMovementMode"] {
                    if let Some(pr) = u.prop(mc, f) { println!("  {f} f32 {:?} u8 {:?}", u.p.read::<f32>(m + pr.offset as u64), u.p.read::<u8>(m + pr.offset as u64)); }
                }
            }
        }
        let sets = u.find_by_classes(|n| n.starts_with("UWE") && n.ends_with("AttributeSet"));
        for s in sets.into_iter().filter(|&s| u.outer_of(s).is_some_and(|o| players.contains(&o))) {
            let cls = u.class_of(s).unwrap();
            println!("== {} 0x{s:X} outer 0x{:X}", u.name_of(cls).unwrap_or_default(), u.outer_of(s).unwrap());
            for pr in u.props(cls).into_iter().filter(|p| p.kind == "StructProperty") {
                let a = s + pr.offset as u64;
                println!("  {} base {:?} current {:?}", pr.name, u.p.read::<f32>(a + 8), u.p.read::<f32>(a + 12));
            }
        }
        return;
    }
    // Dev aid: `hotswap --mono <process name part> <Class> [static field]` lists a Unity class's
    // fields through Mono, and the object a static field points to.
    if let Some(i) = std::env::args().position(|a| a == "--mono") {
        let a: Vec<String> = std::env::args().collect();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().to_lowercase().contains(&a[i + 1].to_lowercase())).map(|p| p.pid().as_u32());
        let mut m = match pid.map(mono::Mono::attach) { Some(Ok(m)) => m, Some(Err(e)) => return println!("attach failed: {e}"), None => return println!("not running") };
        println!("attached");
        let Some(cls_name) = a.get(i + 2) else { return };
        let (ns, name) = cls_name.rsplit_once('.').unwrap_or(("", cls_name));
        let Some(c) = m.class(ns, name) else { return println!("class {cls_name} not found") };
        if a.get(i + 3).map(|s| s.as_str()) == Some("enum") {
            for (n, v) in m.enum_values(c) { println!("  {n} = {v}"); }
            return;
        }
        for (n, off, ty) in m.fields_of(c) { println!("  {n} @0x{off:X} : {ty}"); }
        if let Some(f) = a.get(i + 3) {
            let addr = m.static_addr(c, f);
            println!("static {f} at {addr:X?} = {:X?}", addr.and_then(|x| m.p.ptr(x)));
        }
        return;
    }
    // Dev aid: `hotswap --scrap-test <survival folder copy>` adds and removes the bridge.
    if let Some(i) = std::env::args().position(|a| a == "--scrap-test") {
        let dir = PathBuf::from(std::env::args().nth(i + 1).unwrap_or_default());
        println!("install: {:?}", scrap::install(&dir));
        scrap::uninstall(&dir);
        println!("removed");
        return;
    }
    // Dev aid: `hotswap --scrap-map <install> <out dir>` renders the map from a saved world.
    if let Some(i) = std::env::args().position(|a| a == "--scrap-map") {
        let a: Vec<String> = std::env::args().collect();
        println!("{:?}", scrap::ensure_map(&PathBuf::from(&a[i + 1]), &PathBuf::from(&a[i + 2])));
        return;
    }
    // Dev aid: `hotswap --ksp-peek` prints what the KSP trainer sees.
    if std::env::args().any(|a| a == "--ksp-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy() == "KSP_x64.exe").map(|p| p.pid().as_u32());
        match pid.map(mono::Mono::attach) { Some(Ok(m)) => ksp::peek(m), Some(Err(e)) => println!("attach failed: {e}"), None => println!("not running") }
        return;
    }
    // Dev aid: `hotswap --sn1-peek` prints Subnautica's live player speed values.
    if std::env::args().any(|a| a == "--sn1-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy() == "Subnautica.exe").map(|p| p.pid().as_u32());
        let Some(Ok(mut m)) = pid.map(mono::Mono::attach) else { return println!("not attached") };
        let pc_cls = m.class("", "Player").unwrap();
        let player = m.static_addr(pc_cls, "main").and_then(|a| m.p.ptr(a)).unwrap();
        let o = m.field(pc_cls, "<playerController>k__BackingField").unwrap() as u64;
        let pc = m.p.ptr(player + o).unwrap();
        let c = m.class("", "PlayerController").unwrap();
        for (n, off, ty) in m.fields_of(c) { if ty.contains("Single") || ty.contains("Motor") { println!("PC {n} = {:?} {:X?}", m.p.read::<f32>(pc + off as u64), m.p.ptr(pc + off as u64)); } }
        let (uwo, ao) = (m.field(c, "underWaterController").unwrap() as u64, m.field(c, "activeController").unwrap() as u64);
        let uw = m.p.ptr(pc + uwo).unwrap();
        let act = m.p.ptr(pc + ao);
        println!("underwater 0x{uw:X} active {act:X?}");
        let uc = m.class("", "UnderwaterMotor").unwrap();
        for (n, off, ty) in m.fields_of(uc) { if off != 0 && ty.contains("Single") { println!("UW {n} = {:?}", m.p.read::<f32>(uw + off as u64)); } }
        let mc = m.class("", "PlayerMotor").unwrap();
        for (n, off, ty) in m.fields_of(mc) { if ty.contains("Single") { println!("PM {n} = {:?}", m.p.read::<f32>(uw + off as u64)); } }
        return;
    }
    // Dev aid: `hotswap --sn1-map <out.jpg>` renders Subnautica's terrain heights.
    if let Some(out) = std::env::args().skip_while(|a| a != "--sn1-map").nth(1) {
        let t = std::time::Instant::now();
        let dir = std::path::Path::new(r"E:\SteamLibrary\steamapps\common\Subnautica\Subnautica_Data\StreamingAssets\SNUnmanagedData\Build18");
        if out == "stats" { return println!("{}", sn1map::material_stats(dir).unwrap_or_else(|e| e)); }
        println!("{:?} in {:?}", sn1map::ensure(dir, std::path::Path::new(&out), sn1map::Style::Satellite), t.elapsed());
        return;
    }
    // Dev aid: `hotswap --repo-peek` prints the R.E.P.O. player's and enemies' positions.
    if std::env::args().any(|a| a == "--repo-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy() == "REPO.exe").map(|p| p.pid().as_u32());
        let Some(Ok(mut m)) = pid.map(mono::Mono::attach) else { return println!("not attached") };
        let pa = m.class("", "PlayerAvatar").unwrap();
        let avatar = m.static_addr(pa, "instance").and_then(|a| m.p.ptr(a)).unwrap();
        let (to, co) = (m.field(pa, "playerTransform").unwrap() as u64, m.field(pa, "clientPositionCurrent").unwrap() as u64);
        let t = m.p.ptr(avatar + to).unwrap();
        let cp: Vec<f32> = (0..3).map(|i| m.p.read::<f32>(avatar + co + i * 4).unwrap()).collect();
        println!("player transform {:?} clientPositionCurrent {cp:?}", m.transform_pos(t));
        let ed = m.class("", "EnemyDirector").unwrap();
        let dir = m.static_addr(ed, "instance").and_then(|a| m.p.ptr(a)).unwrap();
        let lo = m.field(ed, "enemiesSpawned").unwrap() as u64;
        let list = m.p.ptr(dir + lo).unwrap();
        let (items, n) = (m.p.ptr(list + 0x10).unwrap(), m.p.read::<i32>(list + 0x18).unwrap());
        let ep = m.class("", "EnemyParent").unwrap();
        let (eo, no, so) = (m.field(ep, "Enemy").unwrap() as u64, m.field(ep, "enemyName").unwrap() as u64, m.field(ep, "Spawned").unwrap() as u64);
        let en = m.class("", "Enemy").unwrap();
        let ct = m.field(en, "CenterTransform").unwrap() as u64;
        for i in 0..n as u64 {
            let Some(par) = m.p.ptr(items + 0x20 + i * 8) else { continue };
            let name = m.p.ptr(par + no).map(|s| { let len = m.p.read::<i32>(s + 0x10).unwrap_or(0).clamp(0, 64) as usize; let mut b = vec![0u8; len * 2]; m.p.read_into(s + 0x14, &mut b); String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()) });
            let spawned = m.p.read::<u8>(par + so);
            let pos = m.p.ptr(par + eo).and_then(|e| m.p.ptr(e + ct)).and_then(|t| m.transform_pos(t));
            println!("  {name:?} spawned={spawned:?} at {pos:?}");
        }
        return;
    }
    // Dev aid: `hotswap --world-scan` prints what the map would draw.
    if std::env::args().any(|a| a == "--world-scan") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        let t = std::time::Instant::now();
        let w = serde_json::to_value(world::scan(&u)).unwrap();
        if let Some(out) = std::env::args().skip_while(|a| a != "--world-scan").nth(1) {
            let _ = fs::write(out, w.to_string());
        }
        println!("scan {:?}", t.elapsed());
        for k in ["nodes", "things", "buildings", "lines"] {
            let a = w[k].as_array().unwrap();
            println!("{k}: {} e.g. {}", a.len(), a.iter().take(3).map(|v| v.to_string()).collect::<Vec<_>>().join(" | "));
        }
        let mut res: std::collections::BTreeMap<String, usize> = Default::default();
        for n in w["nodes"].as_array().unwrap() { *res.entry(format!("{} {} p{}", n["kind"], n["res"], n["purity"])).or_default() += 1; }
        println!("{res:?}");
        return;
    }
    // Dev aid: `hotswap --world-peek` counts map-worthy actor classes and shows key properties.
    if std::env::args().any(|a| a == "--world-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        let t = std::time::Instant::now();
        let mut counts: std::collections::BTreeMap<String, (usize, u64)> = Default::default();
        for o in u.find_by_classes(|n| n.starts_with("Build_") || n.contains("Resource") || n.contains("Fracking") || n.contains("DropPod") || n.contains("Crystal") || n.contains("WAT") || n.contains("Lightweight")) {
            let c = u.class_of(o).unwrap();
            let e = counts.entry(u.name_of(c).unwrap_or_default()).or_insert((0, o));
            e.0 += 1;
        }
        println!("scan {:?}", t.elapsed());
        for (k, (n, o)) in &counts { println!("{n:6} {k}  e.g. 0x{o:X}"); }
        for want in ["Build_ConveyorBeltMk1_C", "BP_ResourceNode_C", "Build_PowerLine_C", "FGLightweightBuildableSubsystem", "BP_DropPod_C"] {
            if let Some((_, o)) = counts.get(want) {
                let c = u.class_of(*o).unwrap();
                let names: Vec<String> = u.props(c).into_iter().filter(|p| !p.owner.starts_with("Actor") && !p.owner.starts_with("Object")).map(|p| format!("{}:{}@{:X}", p.name, p.kind, p.offset)).collect();
                println!("== {want}: {}", names.join(", "));
            }
        }
        return;
    }
    // Dev aid: `hotswap --tp-peek` finds every copy of the player's location in its components.
    if std::env::args().any(|a| a == "--tp-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        for p in u.find_by_class("Char_Player_C") {
            let cls = u.class_of(p).unwrap();
            if u.prop(cls, "Controller").and_then(|o| u.p.ptr(p + o.offset as u64)).is_none() { continue }
            let root = u.prop(cls, "RootComponent").and_then(|o| u.p.ptr(p + o.offset as u64)).unwrap();
            let rc = u.class_of(root).unwrap();
            let rel = u.prop(rc, "RelativeLocation").unwrap().offset as u64;
            let here: Vec<f64> = (0..3).map(|i| u.p.read::<f64>(root + rel + i * 8).unwrap()).collect();
            println!("player 0x{p:X} root 0x{root:X} {} RelativeLocation +0x{rel:X} = {here:?}", u.name_of(rc).unwrap_or_default());
            let m = u.prop(cls, "CharacterMovement").and_then(|o| u.p.ptr(p + o.offset as u64)).unwrap();
            for (label, obj) in [("root", root), ("movement", m), ("player", p)] {
                let mut buf = vec![0u8; 0x1000];
                u.p.read_into(obj, &mut buf);
                for o in (0..0x1000 - 24).step_by(8) {
                    let d = |k: usize| f64::from_le_bytes(buf[o + k * 8..o + k * 8 + 8].try_into().unwrap());
                    if (d(0) - here[0]).abs() < 2000.0 && (d(1) - here[1]).abs() < 2000.0 && (d(2) - here[2]).abs() < 2000.0 {
                        println!("  {label} +0x{o:X}: {:?}", [d(0), d(1), d(2)]);
                    }
                }
                for pr in u.props(u.class_of(obj).unwrap()).iter().filter(|x| x.name.contains("Location") || x.name.contains("Transform")) {
                    println!("    prop {label}.{} +0x{:X} {}", pr.name, pr.offset, pr.kind);
                }
            }
        }
        return;
    }
    // Dev aid: `hotswap --trainer-peek` prints the player's live movement values without writing.
    if std::env::args().any(|a| a == "--trainer-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        for p in u.find_by_class("Char_Player_C") {
            let cls = u.class_of(p).unwrap();
            let ctrl = u.prop(cls, "Controller").and_then(|o| u.p.ptr(p + o.offset as u64));
            let Some(m) = u.prop(cls, "CharacterMovement").and_then(|o| u.p.ptr(p + o.offset as u64)) else { continue };
            let mc = u.class_of(m).unwrap();
            let read = |n: &str| u.prop(mc, n).and_then(|o| u.p.read::<f32>(m + o.offset as u64));
            println!("player 0x{p:X} ctrl={ctrl:X?} walk={:?} sprint={:?} jump={:?} gravity={:?}", read("MaxWalkSpeed"), read("mMaxSprintSpeed"), read("JumpZVelocity"), read("GravityScale"));
        }
        for c in ["BP_GameState_C", "FGGameState"] {
            for gs in u.find_by_class(c) {
                let cls = u.class_of(gs).unwrap();
                let flags: Vec<String> = ["mCheatNoCost", "mCheatNoPower", "mCheatNoFuel", "mCheatTurboProductionMode", "mCheatTurboBuildMode"]
                    .iter().map(|n| format!("{n}={:?}", u.prop(cls, n).and_then(|o| u.p.read::<u8>(gs + o.offset as u64)))).collect();
                println!("{c} 0x{gs:X}: {}", flags.join(" "));
            }
        }
        return;
    }
    // Dev aid: `hotswap --call-test` adds 10 iron plates to the player's inventory via AddStack.
    if std::env::args().any(|a| a == "--call-test") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        println!("ProcessEvent = 0x{:X}", u.process_event);
        let Some(f) = u.func("FGInventoryComponent", "AddStack") else { println!("AddStack not found"); return };
        println!("AddStack 0x{:X} params {:?}", f.addr, f.params);
        let plate = u.find_class("Desc_IronPlate_C").unwrap_or(0);
        let player = u.find_by_class("Char_Player_C").into_iter().find(|&p| {
            u.class_of(p).and_then(|c| u.prop(c, "Controller")).and_then(|o| u.p.ptr(p + o.offset as u64)).is_some()
        });
        let Some(player) = player else { println!("no player"); return };
        let inv = u.class_of(player).and_then(|c| u.prop(c, "mInventory")).and_then(|o| u.p.ptr(player + o.offset as u64)).unwrap_or(0);
        let off = |n: &str| f.params.iter().find(|p| p.0 == n).map(|p| p.1);
        let mut params = vec![0u8; f.size as usize];
        let st = off("stack").or_else(|| off("Stack")).unwrap_or(0) as usize;
        params[st + 8..st + 16].copy_from_slice(&plate.to_le_bytes());
        params[st + 0x28..st + 0x2C].copy_from_slice(&10i32.to_le_bytes());
        match u.call(u.process_event, inv, &f, &params) {
            Ok(out) => {
                let ret = off("ReturnValue").map(|o| i32::from_le_bytes(out[o as usize..o as usize + 4].try_into().unwrap()));
                println!("called OK, added {ret:?}");
            }
            Err(e) => println!("call failed: {e}"),
        }
        return;
    }
    // Dev aid: `hotswap --machine-peek <Build_X_C>` shows a machine's recipe and input slots.
    if let Some(i) = std::env::args().position(|a| a == "--machine-peek") {
        let cls_name = std::env::args().nth(i + 1).unwrap_or_default();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        let recipe_filter = std::env::args().nth(i + 2).unwrap_or_default();
        let all = u.find_by_class(&cls_name);
        let pick: Vec<u64> = all.into_iter().filter(|&m| recipe_filter.is_empty() || u.class_of(m).and_then(|c| u.prop(c, "mCurrentRecipe")).and_then(|p| u.p.ptr(m + p.offset as u64)).and_then(|r| u.name_of(r)).is_some_and(|n| n.contains(&recipe_filter))).take(2).collect();
        for m in pick {
            let cls = u.class_of(m).unwrap();
            let get = |n: &str| u.prop(cls, n).map(|p| m + p.offset as u64);
            let recipe = get("mCachedRecipe").and_then(|a| u.p.ptr(a));
            let cur = get("mCurrentRecipe").and_then(|a| u.p.ptr(a)).and_then(|c| u.name_of(c));
            println!("machine 0x{m:X} current={cur:?} cached={recipe:X?} ({:?})", recipe.and_then(|r| u.name_of(r)));
            let f = |n: &str| get(n).and_then(|a| u.p.read::<f32>(a));
            let b = |n: &str| get(n).and_then(|a| u.p.read::<u8>(a));
            println!("  progress={:?} potential cur={:?} pending={:?} max={:?} paused={:?}", f("mCurrentManufacturingProgress"), f("mCurrentPotential"), f("mPendingPotential"), f("mMaxPotential"), b("mIsProductionPaused"));
            if let Some(r) = recipe {
                let rc = u.class_of(r).unwrap();
                if let Some(p) = u.prop(rc, "mIngredients") {
                    let (data, n) = (u.p.ptr(r + p.offset as u64).unwrap_or(0), u.p.read::<i32>(r + p.offset as u64 + 8).unwrap_or(0));
                    for k in 0..n.clamp(0, 8) as u64 {
                        let item = u.p.read::<u64>(data + k * 16).unwrap_or(0);
                        println!("  needs {} x{}", u.name_of(item).unwrap_or_default(), u.p.read::<i32>(data + k * 16 + 8).unwrap_or(0));
                    }
                }
            }
            for (label, prop) in [("out", "mOutputInventory")] {
                if let Some(inv) = get(prop).and_then(|a| u.p.ptr(a)) {
                    let ic = u.class_of(inv).unwrap();
                    let arr = inv + u.prop(ic, "mInventoryStacks").unwrap().offset as u64;
                    let (data, n) = (u.p.ptr(arr).unwrap_or(0), u.p.read::<i32>(arr + 8).unwrap_or(0));
                    for k in 0..n.clamp(0, 8) as u64 {
                        let s = data + k * 0x30;
                        let item = u.p.read::<u64>(s + 8).unwrap_or(0);
                        println!("  {label} {k}: {} x{}", u.name_of(item).unwrap_or("empty".into()), u.p.read::<i32>(s + 0x28).unwrap_or(0));
                    }
                }
            }
            if let Some(inv) = get("mInputInventory").and_then(|a| u.p.ptr(a)) {
                let ic = u.class_of(inv).unwrap();
                let arr = inv + u.prop(ic, "mInventoryStacks").unwrap().offset as u64;
                let (data, n) = (u.p.ptr(arr).unwrap_or(0), u.p.read::<i32>(arr + 8).unwrap_or(0));
                for k in 0..n.clamp(0, 8) as u64 {
                    let s = data + k * 0x30;
                    let item = u.p.read::<u64>(s + 8).unwrap_or(0);
                    println!("  slot {k}: {} x{}", u.name_of(item).unwrap_or("empty".into()), u.p.read::<i32>(s + 0x28).unwrap_or(0));
                }
            }
        }
        return;
    }
    if std::env::args().any(|a| a == "--trainer-test") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        match sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")) {
            Some(p) => trainer::debug(p.pid().as_u32()),
            None => println!("Satisfactory isn't running"),
        }
        return;
    }
    // Dev aid: `hotswap --ue-props <ClassName> [filter]` lists a class's properties in the running game.
    if let Some(i) = std::env::args().position(|a| a == "--ue-props") {
        let a: Vec<String> = std::env::args().collect();
        let filter = a.get(i + 2).map(|s| s.to_lowercase()).unwrap_or_default();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|pid| ue::objects::Ue::attach(pid).ok()) else { println!("no game"); return };
        let Some(cls) = u.find_class(&a[i + 1]) else { println!("class not found"); return };
        for p in u.props(cls).iter().filter(|p| p.name.to_lowercase().contains(&filter)) {
            println!("  {:<40} {:<18} +0x{:<5X} {}", p.name, p.kind, p.offset, p.owner);
        }
        return;
    }
    // Dev aid: `hotswap --ue-dump [ClassName]` attaches to a running Unreal game and lists objects.
    if let Some(i) = std::env::args().position(|a| a == "--ue-dump") {
        let class = std::env::args().nth(i + 1);
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let Some(pid) = sys.processes().values().find(|p| p.name().to_string_lossy().contains("Win64-Shipping")).map(|p| p.pid().as_u32()) else {
            println!("No *-Win64-Shipping.exe game running");
            return;
        };
        match ue::objects::Ue::attach(pid) {
            Err(e) => println!("attach failed: {e}"),
            Ok(u) => {
                println!("attached to {pid}: {} objects", u.count());
                for i in 0..12 {
                    if let Some(o) = u.object(i) {
                        let cls = u.class_of(o).and_then(|c| u.name_of(c)).unwrap_or_default();
                        println!("  [{i}] {} : {cls}", u.name_of(o).unwrap_or_default());
                    }
                }
                if let Some(c) = class {
                    let found = u.find_by_class(&c);
                    println!("{c}: {} found", found.len());
                    for o in found.iter().take(5) {
                        let outer = u.outer_of(*o).and_then(|x| u.name_of(x)).unwrap_or_default();
                        println!("  0x{o:X} {} (in {outer})", u.name_of(*o).unwrap_or_default());
                    }
                }
            }
        }
        return;
    }
    let start_hidden = std::env::args().any(|a| a == "--minimized");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .manage(watch::Watch::default())
        .manage(Hotkeys::default())
        .manage(Launches::default())
        .manage(trainer::Trainer::default())
        .manage(CloseToTray(AtomicBool::new(true)))
        .manage(Mini::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    let combo = app.state::<Hotkeys>().0.lock().unwrap().get(&shortcut.id()).cloned();
                    if let Some(combo) = combo {
                        // Releases matter for keys you hold, like the cheat sheet.
                        let name = if event.state() == ShortcutState::Pressed { "hotkey" } else { "hotkey-up" };
                        let _ = app.emit(name, combo);
                    }
                })
                .build(),
        )
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
                remember_geometry(window);
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && window.app_handle().state::<CloseToTray>().0.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            migrate_old_data(app);
            app.state::<watch::Watch>().start(app.handle().clone());
            let watch = app.state::<watch::Watch>().inner().clone();
            app.state::<trainer::Trainer>().start(app.handle().clone(), move |id| watch.pid_of(id));
            build_tray(app)?;
            fit_window(app, !start_hidden);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_data, save_data, scan_library, running_game, launch_game, set_hotkeys,
            fetch_home, close_game, trainer_set, trainer_action, set_close_to_tray, get_autostart, set_autostart,
            map_show, map_hide, map_world, map_background, check_update, install_update, app_version, overlay_notify, overlay_close, radar_show, set_mini, fmods_list, fmods_set_enabled, fmods_remove, fmods_install, fmods_downloads, fmods_pick, nexus_search, mods_state, mods_search, mods_install, mods_uninstall, mods_set_enabled, mods_updates, map_player, map_teleport, map_pins_load, map_pins_save
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hotswap");
}
