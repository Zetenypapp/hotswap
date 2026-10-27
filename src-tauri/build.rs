/// App commands, so capabilities can grant them per window (the map window gets only `map_*`).
const COMMANDS: &[&str] = &[
    "load_data", "save_data", "scan_library", "running_game", "launch_game", "set_hotkeys",
    "fetch_home", "close_game", "trainer_set", "trainer_action", "set_close_to_tray", "get_autostart", "set_autostart",
    "open_map", "map_player", "map_teleport", "map_pins_load", "map_pins_save",
];

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)))
        .expect("failed to run tauri-build");
}
