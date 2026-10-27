#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appinfo;
mod epic;
mod gog;
mod home;
mod iostore;
mod mapimage;
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
async fn map_background(app: AppHandle, watch: State<'_, watch::Watch>, game: String) -> Result<MapBackground, String> {
    if game != trainer::SATISFACTORY {
        return Err("No map for this game".into());
    }
    let install = watch.install_of(&game).ok_or("Game isn't installed")?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("map").join(&game);
    let d = dir.clone();
    tauri::async_runtime::spawn_blocking(move || mapimage::ensure(&install, &d)).await.map_err(|e| e.to_string())??;
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
    // Dev aid: `hotswap --map-image <out dir>` builds the map background from the game files.
    if let Some(out) = std::env::args().skip_while(|a| a != "--map-image").nth(1) {
        let t = std::time::Instant::now();
        let install = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\Satisfactory");
        println!("{:?} in {:?}", mapimage::ensure(install, std::path::Path::new(&out)), t.elapsed());
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
            map_show, map_hide, map_world, map_background, map_player, map_teleport, map_pins_load, map_pins_save
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hotswap");
}
