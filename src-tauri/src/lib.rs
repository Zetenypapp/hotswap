//! HotSwap's code, shared by its two programs (see ARCHITECTURE.md):
//! - hotswap.exe (main.rs, app.rs): the app. It never reads or writes other programs' memory.
//! - hotswap-engine.exe (bin/hsengine.rs, engine.rs): the mod engine, started by the app when a
//!   game with mods runs. All game memory code (trainer.rs and the game readers) runs there.
//! Each program only contains the code it uses.

pub mod app;
pub mod appinfo;
pub mod boost;
pub mod bridge;
pub mod catalog;
pub mod cdp;
pub mod dev;
pub mod discord;
pub mod engine;
pub mod engine_host;
pub mod engines;
pub mod epic;
pub mod ets2;
pub mod extra;
pub mod farming;
pub mod fps;
pub mod fthr;
pub mod gog;
pub mod home;
pub mod il2cpp;
pub mod iostore;
pub mod ksp;
pub mod logs;
pub mod mapimage;
pub mod modfiles;
pub mod modloader;
pub mod mono;
pub mod overlay;
pub mod pad;
pub mod remote;
pub mod repo;
pub mod scrap;
pub mod sn1;
pub mod sn1map;
pub mod sn2;
pub mod sources;
pub mod spacedock;
pub mod stats;
pub mod steam;
pub mod stormworks;
pub mod thunderstore;
pub mod trainer;
pub mod ue;
pub mod vdf;
pub mod voice;
pub mod watch;
pub mod world;
