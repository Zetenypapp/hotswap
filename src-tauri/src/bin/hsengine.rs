//! hotswap-engine.exe: HotSwap's mod engine (engine.rs), started by the app. With a `--flag` it
//! runs a dev aid instead (dev.rs).

fn main() {
    hotswap_core::logs::catch_panics();
    if std::env::args().nth(1).is_some_and(|a| a.starts_with("--")) {
        hotswap_core::dev::run();
        return;
    }
    hotswap_core::engine::serve();
}
