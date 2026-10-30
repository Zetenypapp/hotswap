/// App commands, so capabilities can grant them per window (the map window gets only `map_*`).
const COMMANDS: &[&str] = &[
    "load_data", "save_data", "scan_library", "running_game", "launch_game", "set_hotkeys",
    "fetch_home", "fetch_source", "log_ui", "export_logs", "close_game", "trainer_set", "trainer_action", "set_close_to_tray", "get_autostart", "set_autostart",
    "map_show", "map_hide", "map_world", "map_background", "check_update", "install_update", "app_version", "overlay_notify", "overlay_close", "overlay_menu", "overlay_input", "overlay_ready", "mods_release", "ts_search", "ts_list", "ts_install", "ts_install_file", "ts_set_enabled", "ts_remove", "voice_start", "voice_options", "voice_stop", "voice_state", "voice_send", "system_stats", "system_stats_full", "discord_presence", "radar_show", "set_mini", "fmods_list", "fmods_set_enabled", "fmods_remove", "fmods_install", "fmods_downloads", "fmods_pick", "nexus_search", "mods_state", "mods_search", "mods_install", "mods_uninstall", "mods_set_enabled", "mods_updates", "map_player", "map_teleport", "map_pins_load", "map_pins_save",
];

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)))
        .expect("failed to run tauri-build");
}
