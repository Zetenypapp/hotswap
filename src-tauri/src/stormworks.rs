//! Stormworks: Build and Rescue (Steam app 573090). No memory reading: HotSwap puts a small addon
//! (assets/sw_addon.lua) in the game's addon folder, and the game runs it in worlds where it's
//! enabled. The addon asks HotSwap, over HTTP on localhost (the only place the game's addon API can
//! reach), which mods are on, and applies them with the game's own settings and money functions.
//! The game's addon API is official and kept stable, so game updates don't break the mods.

use crate::trainer::log;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

pub const STORMWORKS: &str = "573090";
/// The port the addon asks; must match PORT in sw_addon.lua.
const PORT: u16 = 47823;
const ADDON: &str = include_str!("../assets/sw_addon.lua");
const PLAYLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<playlist folder_path="data/missions/HotSwap" file_store="4" name="HotSwap">
	<locations/>
</playlist>
"#;

/// What the addon gets: the reply text, and when it last asked.
struct Shared {
    reply: String,
    seen: Option<Instant>,
}
static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();

fn shared() -> &'static Mutex<Shared> {
    SHARED.get_or_init(|| {
        // The server runs for the rest of the app's life; it only answers the addon's one path.
        match TcpListener::bind(("127.0.0.1", PORT)) {
            Ok(l) => {
                thread::spawn(move || {
                    for mut s in l.incoming().flatten() {
                        let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut buf = [0u8; 2048];
                        let n = s.read(&mut buf).unwrap_or(0);
                        let req = String::from_utf8_lossy(&buf[..n]);
                        let path = req.split_whitespace().nth(1).unwrap_or("");
                        let body = if path.starts_with("/hs") {
                            let mut sh = shared().lock().unwrap();
                            sh.seen = Some(Instant::now());
                            sh.reply.clone()
                        } else {
                            String::new()
                        };
                        let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
                    }
                });
            }
            Err(e) => log(&format!("stormworks: can't listen on {PORT}: {e}")),
        }
        Mutex::new(Shared { reply: String::new(), seen: None })
    })
}

/// %APPDATA%\Stormworks, where the game keeps saves and the player's addons.
fn data_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Stormworks"))
}

/// Puts the addon in the game's addon folder (or updates it). Only when the game has made its
/// folder, so nothing is created for people who never started it.
pub fn install() -> Result<(), String> {
    let root = data_dir().ok_or("No APPDATA folder")?;
    if !root.is_dir() {
        return Err("Start Stormworks once first".into());
    }
    let dir = root.join(r"data\missions\HotSwap");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, text) in [("playlist.xml", PLAYLIST), ("script.lua", ADDON)] {
        let path = dir.join(name);
        if fs::read_to_string(&path).ok().as_deref() != Some(text) {
            fs::write(&path, text).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub struct Session {
    run: u32,
    /// Times each action was pressed this game run.
    actions: HashMap<String, u64>,
}

impl Session {
    pub fn attach() -> Result<Session, String> {
        install()?;
        shared();
        let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() | 1).unwrap_or(1);
        Ok(Session { run, actions: HashMap::new() })
    }

    /// Ok(true) while the addon is asking (a world with it is open).
    pub fn tick(&mut self, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Result<bool, String> {
        for a in actions {
            *self.actions.entry(a.clone()).or_default() += 1;
        }
        let mut reply = format!("hs\nrun {}\n", self.run);
        for (k, v) in values.into_iter().flatten() {
            reply += &format!("m {k} {v}\n");
        }
        for (k, n) in &self.actions {
            reply += &format!("a {k} {n}\n");
        }
        let mut sh = shared().lock().unwrap();
        sh.reply = reply;
        Ok(sh.seen.is_some_and(|t| t.elapsed() < Duration::from_secs(3)))
    }
}
