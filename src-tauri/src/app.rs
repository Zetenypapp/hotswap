//! The app (hotswap.exe): settings, main window, tray, hotkeys, library, launching games and
//! updates, and every command the interface can call. Game mods run in the separate mod engine
//! (engine.rs, started by engine_host.rs). See ARCHITECTURE.md.

use crate::overlay::LOW_MEMORY;
// Command groups that share this file's imports.
#[path = "cmd_discord.rs"]
mod cmd_discord;
#[path = "cmd_maps.rs"]
mod cmd_maps;
#[path = "cmd_mods.rs"]
mod cmd_mods;
#[allow(unused_imports)]
use crate::*;



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
        steam::Launch::Web { path, dir, app_id } => std::process::Command::new(path)
            .current_dir(dir)
            .arg(format!("--remote-debugging-port={}", cdp::PORT))
            .env("SteamAppId", app_id.to_string())
            .env("SteamGameId", app_id.to_string())
            // Set when HotSwap itself was started from an Electron app; it would make the game run as plain Node.
            .env_remove("ELECTRON_RUN_AS_NODE")
            .spawn(),
        steam::Launch::None => return Err("HotSwap doesn't know how to start this game".into()),
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
/// A line from the interface for the log (errors and failed commands).
#[tauri::command]
fn log_ui(msg: String) {
    logs::write("ui", &msg.chars().take(2000).collect::<String>());
}

/// Packs the logs into a zip (small: under 5 MB, for Discord) and shows it in Explorer.
#[tauri::command]
async fn export_logs(small: bool, summary: String) -> Result<String, String> {
    let path = tauri::async_runtime::spawn_blocking(move || logs::export(small, &summary)).await.map_err(|e| e.to_string())??;
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    Ok(path.display().to_string())
}

/// A Home panel's custom source: a feed, a JSON list or a website (see sources.rs).
#[tauri::command]
async fn fetch_source(url: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || sources::fetch(&url)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn trainer_set(trainer: State<engine_host::Engine>, watch: State<watch::Watch>, game: Option<String>, paused: bool, values: HashMap<String, f64>, engine: Option<String>, specs: Option<HashMap<String, Vec<engines::Spec>>>, guard: Option<Vec<engines::Spec>>) {
    let install = game.as_deref().and_then(|g| watch.install_of(g));
    trainer.set(game, paused, values, engine, specs.unwrap_or_default(), guard.unwrap_or_default(), install);
}

/// Whether a game's one-time setup is done (the guide on its page): "scripthookv" for GTA V.
#[tauri::command]
fn setup_check(watch: State<watch::Watch>, id: String, kind: String) -> bool {
    let Some(dir) = watch.install_of(&id) else { return false };
    match kind.as_str() {
        "scripthookv" => bridge::has_script_hook(&dir),
        _ => false,
    }
}

/// One-shot mods like "Add 1,000 Coupons".
#[tauri::command]
fn trainer_action(trainer: State<engine_host::Engine>, id: String) {
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

/// CPU and memory use in percent, for the overlay HUD.
#[tauri::command]
fn system_stats(stats: State<stats::Stats>) -> (f32, f32) {
    stats.basic()
}

/// Everything the overlay's System tab graphs (see stats.rs).
#[tauri::command]
fn system_stats_full(stats: State<stats::Stats>, watch: State<watch::Watch>, game: Option<String>) -> stats::Snapshot {
    stats.read(game.and_then(|g| watch.stats_pid_of(&g)))
}

#[derive(serde::Serialize)]
struct UpdateInfo {
    version: String,
    notes: String,
}

/// A newer HotSwap release on GitHub, if there is one (signed; checked against the built-in key).
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = updater(&app)?.check().await.map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo { version: shown_version(&u.version), notes: u.body.clone().unwrap_or_default() }))
}

/// HotSwap versions can have a fourth number (0.3.5.1). Versions must be semver, so it's written
/// as build metadata (0.3.5+1), which semver itself ignores when comparing: this compares it too.
fn version_key(v: &semver::Version) -> (u64, u64, u64, u64) {
    (v.major, v.minor, v.patch, v.build.as_str().parse().unwrap_or(0))
}

/// How a version is shown: 0.3.5+1 as 0.3.5.1.
fn shown_version(v: &str) -> String {
    v.replacen('+', ".", 1)
}

fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    use tauri_plugin_updater::UpdaterExt;
    app.updater_builder()
        .version_comparator(|current, release| version_key(&release.version) > version_key(&current))
        .build()
        .map_err(|e| e.to_string())
}

/// Downloads and installs the newest release, then restarts into it.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    let update = updater(&app)?.check().await.map_err(|e| e.to_string())?.ok_or("Already up to date")?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    app.restart();
}

#[tauri::command]
fn app_version(app: AppHandle) -> String {
    shown_version(&app.package_info().version.to_string())
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

/// Tells WebView2 how much memory a window may keep: low while it's hidden in the tray, so
/// WebView2 frees what it can, and normal again once it's shown.
pub fn webview_memory(win: &tauri::WebviewWindow, low: bool) {
    #[cfg(windows)]
    let _ = win.with_webview(move |wv| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL};
        use windows_core::Interface;
        if let Ok(core) = wv.controller().CoreWebView2() {
            if let Ok(c) = core.cast::<ICoreWebView2_19>() {
                let _ = c.SetMemoryUsageTargetLevel(if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL });
            }
        }
    });
}

fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        webview_memory(&win, LOW_MEMORY.load(Ordering::Relaxed));
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
    let open = MenuItem::with_id(app, "open", "Open HotSwap", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit HotSwap", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("HotSwap")
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

pub fn run() {
    logs::catch_panics();
    // Low memory mode (a setting), read before any webview starts. WebView2 runs leaner:
    // - no graphics acceleration, with the GPU and network work inside the browser process
    //   instead of their own processes;
    // - no spare renderer kept warm, no extensions, no background downloads;
    // - a smaller JavaScript heap.
    // Together that's about 150 MB instead of 400+ (see also webview_memory and the UI).
    if let Ok(appdata) = std::env::var("APPDATA") {
        let state = fs::read_to_string(std::path::Path::new(&appdata).join("app.hotswap.desktop").join("state.json")).unwrap_or_default();
        if serde_json::from_str::<serde_json::Value>(state.trim_start_matches('\u{feff}')).ok().and_then(|v| v["lowMemory"].as_bool()) == Some(true) {
            let args = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
            const LEAN: &str = "--disable-gpu --disable-gpu-compositing --in-process-gpu --enable-features=NetworkServiceInProcess2 \
                --disable-features=SpareRendererForSitePerProcess,msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-site-isolation-trials \
                --renderer-process-limit=1 --disable-background-networking --disable-component-update --disable-extensions --js-flags=--max-old-space-size=64";
            std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", format!("{args} {LEAN}").trim());
            LOW_MEMORY.store(true, Ordering::Relaxed);
        }
    }
    let start_hidden = std::env::args().any(|a| a == "--minimized");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .manage(watch::Watch::default())
        .manage(Hotkeys::default())
        .manage(Launches::default())
        .manage(engine_host::Engine::default())
        .manage(boost::Boost::default())
        .manage(CloseToTray(AtomicBool::new(true)))
        .manage(Mini::default())
        .manage(stats::Stats::default())
        .manage(discord::Discord::default())
        .manage(voice::Voice::default())
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
                    if let Some(w) = window.app_handle().get_webview_window("main") {
                        webview_memory(&w, true);
                    }
                }
            }
        })
        .setup(move |app| {
            migrate_old_data(app);
            boost::recover(app.handle());
            app.state::<watch::Watch>().start(app.handle().clone());
            pad::start(app.handle().clone());
            app.state::<engine_host::Engine>().start(app.handle().clone());
            build_tray(app)?;
            logs::write("app", &format!("HotSwap {} started (low memory mode {}, {})", app.package_info().version, LOW_MEMORY.load(Ordering::Relaxed), if start_hidden { "in the tray" } else { "window shown" }));
            fit_window(app, !start_hidden);
            // Low memory mode keeps WebView2's memory target low all the time, not only in the tray.
            if LOW_MEMORY.load(Ordering::Relaxed) {
                if let Some(w) = app.get_webview_window("main") {
                    webview_memory(&w, true);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_data, save_data, scan_library, running_game, launch_game, set_hotkeys, fetch_home, fetch_source, log_ui, export_logs, close_game, trainer_set, setup_check, catalog::catalog_remote, boost::boost_apps, boost::boost_set, boost::boost_clean, boost::boost_hw, trainer_action, set_close_to_tray, get_autostart, set_autostart, cmd_maps::map_show, cmd_maps::map_hide, cmd_maps::map_world, cmd_maps::map_background, check_update, install_update, app_version, overlay::overlay_notify, overlay::overlay_close, overlay::overlay_menu, overlay::overlay_input, overlay::overlay_ready, overlay::overlay_fit, cmd_mods::mods_release, cmd_mods::ts_search, cmd_mods::ts_list, cmd_mods::ts_install, cmd_mods::ts_install_file, cmd_mods::ts_set_enabled, cmd_mods::ts_remove, cmd_discord::voice_start, cmd_discord::voice_options, cmd_discord::voice_stop, cmd_discord::voice_state, cmd_discord::voice_send, system_stats, system_stats_full, fps::fps_allow, fps::fps_status, fthr::clips_status, fthr::clips_command, fthr::clips_open, cmd_discord::discord_presence, overlay::radar_show, set_mini, cmd_mods::fmods_list, cmd_mods::fmods_set_enabled, cmd_mods::fmods_remove, cmd_mods::fmods_install, cmd_mods::fmods_downloads, cmd_mods::fmods_pick, cmd_mods::nexus_search, cmd_mods::mods_state, cmd_mods::mods_search, cmd_mods::mods_install, cmd_mods::mods_uninstall, cmd_mods::mods_set_enabled, cmd_mods::mods_updates, cmd_maps::map_player, cmd_maps::map_teleport, cmd_maps::map_pins_load, cmd_maps::map_pins_save
        ])
        .run(tauri::generate_context!())
        .expect("error while running HotSwap");
}
