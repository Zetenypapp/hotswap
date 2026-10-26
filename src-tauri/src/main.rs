#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appinfo;
mod steam;
mod watch;

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Registered system-wide hotkeys: shortcut ID -> the combo string the UI asked for.
#[derive(Default)]
struct Hotkeys(Mutex<HashMap<u32, String>>);

fn state_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.json"))
}

/// Returns the saved UI state as raw JSON, or None on first launch.
#[tauri::command]
fn load_state(app: AppHandle) -> Result<Option<String>, String> {
    Ok(fs::read_to_string(state_file(&app)?).ok())
}

/// Writes the UI state through a temp file so a crash mid-write can't corrupt it.
#[tauri::command]
fn save_state(app: AppHandle, data: String) -> Result<(), String> {
    let path = state_file(&app)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Finds installed Steam games and their cached artwork, lets the webview load
/// images from Steam's local art cache, and points the game watcher at them.
#[tauri::command]
fn scan_steam(app: AppHandle, watch: State<watch::Watch>) -> steam::Scan {
    let scan = steam::scan();
    if let Some(root) = &scan.root {
        let cache = PathBuf::from(root).join("appcache").join("librarycache");
        let _ = app.asset_protocol_scope().allow_directory(&cache, true);
    }
    watch.set_targets(scan.games.iter().filter_map(|g| g.watch_target()).collect());
    scan
}

/// The game currently running, if any (the UI also gets `game-running` events).
#[tauri::command]
fn running_game(watch: State<watch::Watch>) -> Option<String> {
    watch.running()
}

/// Starts a Steam game through the Steam client.
#[tauri::command]
fn launch_game(id: String) -> Result<(), String> {
    let app_id: u32 = id.parse().map_err(|_| format!("Not a Steam game: {id}"))?;
    std::process::Command::new("explorer.exe")
        .arg(format!("steam://rungameid/{app_id}"))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
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

/// Fits the window to the monitor (at high display scaling 1440x880 can be taller
/// than the screen), then shows it. The window starts hidden to avoid a visible jump.
fn fit_and_show(app: &tauri::App) {
    let Some(win) = app.get_webview_window("main") else { return };
    if let Ok(Some(monitor)) = win.current_monitor() {
        let screen = monitor.size().to_logical::<f64>(monitor.scale_factor());
        let size = tauri::LogicalSize::new((screen.width * 0.9).min(1440.0), (screen.height * 0.88).min(880.0));
        let _ = win.set_size(size);
        let _ = win.center();
    }
    let _ = win.show();
}

fn main() {
    // Dev aid: `tinker --scan` prints what the Steam scanner finds and exits.
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
    tauri::Builder::default()
        .manage(watch::Watch::default())
        .manage(Hotkeys::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let combo = app.state::<Hotkeys>().0.lock().unwrap().get(&shortcut.id()).cloned();
                        if let Some(combo) = combo {
                            let _ = app.emit("hotkey", combo);
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            app.state::<watch::Watch>().start(app.handle().clone());
            fit_and_show(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![load_state, save_state, scan_steam, running_game, launch_game, set_hotkeys])
        .run(tauri::generate_context!())
        .expect("error while running Tinker");
}
