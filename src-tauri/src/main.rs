//! hotswap.exe: the app (app.rs). Game mods run in hotswap-engine.exe (engine.rs).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let a: Vec<String> = std::env::args().collect();
    // The admin half of Settings → FPS counter → Allow (fps::fps_allow): one job, then exit.
    #[cfg(windows)]
    if a.get(1).map(String::as_str) == Some("--fps-allow") {
        std::process::exit(hotswap_core::fps::allow_elevated(a.get(2).map(String::as_str).unwrap_or("")));
    }
    hotswap_core::app::run();
}
