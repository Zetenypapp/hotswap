#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appinfo;
mod steam;

use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

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

/// Finds installed Steam games and their cached artwork, and lets the
/// webview load images from Steam's local art cache.
#[tauri::command]
fn scan_steam(app: AppHandle) -> steam::Scan {
    let scan = steam::scan();
    if let Some(root) = &scan.root {
        let cache = PathBuf::from(root).join("appcache").join("librarycache");
        let _ = app.asset_protocol_scope().allow_directory(&cache, true);
    }
    scan
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
        .setup(|app| {
            fit_and_show(app);
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![load_state, save_state, scan_steam])
        .run(tauri::generate_context!())
        .expect("error while running Tinker");
}
