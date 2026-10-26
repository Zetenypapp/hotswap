#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appinfo;
mod epic;
mod gog;
mod home;
mod steam;
mod ue;
mod vdf;
mod watch;

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

/// Files the UI may read and write in the app data folder.
fn data_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let file = match name {
        "state" => "state.json",
        "home" => "home_cache.json",
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
    if let Ok(Some(monitor)) = win.current_monitor() {
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
        .manage(CloseToTray(AtomicBool::new(true)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
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
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.app_handle().state::<CloseToTray>().0.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            migrate_old_data(app);
            app.state::<watch::Watch>().start(app.handle().clone());
            build_tray(app)?;
            fit_window(app, !start_hidden);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_data, save_data, scan_library, running_game, launch_game, set_hotkeys,
            fetch_home, close_game, set_close_to_tray, get_autostart, set_autostart
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hotswap");
}
