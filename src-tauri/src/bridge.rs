//! Script engines, Ren'Py and Godot, and GTA V story mode: HotSwap adds a small script or plugin the
//! game loads at startup (assets/hotswap.rpy, assets/hotswap_godot.gd, gta-asi for Script Hook V). Twice a second it asks HotSwap over local UDP for
//! the mods, and applies them to the game's own variables by name. Same operations as engines.rs.

use crate::engines::Spec;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::UdpSocket,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

pub const PORT: u16 = 47831;

pub fn is_script(engine: &str) -> bool {
    matches!(engine, "renpy" | "godot" | "gta")
}

/// HotSwap's Script Hook V plugin, built from gta-asi by build.rs.
const GTA_ASI: &[u8] = include_bytes!(env!("HOTSWAP_GTA_ASI"));

/// True when Script Hook V is installed in a GTA V folder.
pub fn has_script_hook(game_dir: &Path) -> bool {
    game_dir.join("ScriptHookV.dll").exists()
}

/// True when BattlEye is running for the game `pid` (GTA V loads it unless started with -nobattleye).
fn battleye(pid: Option<u32>) -> bool {
    pid.is_some_and(|pid| crate::ue::mem::has_module(pid, "BEClient"))
}

const RENPY: &str = include_str!("../assets/hotswap.rpy");
const GODOT: &str = include_str!("../assets/hotswap_godot.gd");

fn socket() -> Option<&'static UdpSocket> {
    static S: OnceLock<Option<UdpSocket>> = OnceLock::new();
    S.get_or_init(|| {
        let s = UdpSocket::bind(("127.0.0.1", PORT)).ok()?;
        s.set_nonblocking(true).ok()?;
        Some(s)
    })
    .as_ref()
}

/// Puts the script where the game loads it. Returns false if the game can't be set up.
pub fn install(engine: &str, game_dir: &Path) -> Result<(), String> {
    let write = |p: PathBuf, s: &str| {
        if std::fs::read_to_string(&p).ok().as_deref() != Some(s) {
            std::fs::write(&p, s).map_err(|e| format!("couldn't add HotSwap to the game: {e}"))?;
        }
        Ok::<(), String>(())
    };
    match engine {
        // Ren'Py loads every .rpy file in the game folder at startup.
        "renpy" => {
            let dir = [game_dir.join("game")].into_iter().chain(std::fs::read_dir(game_dir).into_iter().flatten().flatten().map(|e| e.path().join("game")))
                .find(|d| d.is_dir() && d.parent().is_some_and(|p| p.join("renpy").is_dir()))
                .ok_or("couldn't find the game's folder")?;
            write(dir.join("zz_hotswap.rpy"), RENPY)
        }
        // Godot reads override.cfg next to the game's .pck: it adds our script as an autoload.
        "godot" => {
            let pck = find_pck(game_dir).ok_or("couldn't find the game's .pck")?;
            let dir = pck.parent().unwrap_or(game_dir);
            let v4 = godot_major(&pck).unwrap_or(4) >= 2;
            let script = dir.join("hotswap_bridge.gd");
            write(script.clone(), &godot_script(v4))?;
            let cfg = dir.join("override.cfg");
            let old = std::fs::read_to_string(&cfg).unwrap_or_default();
            if !old.contains("hotswap_bridge.gd") {
                let line = format!("HotSwap=\"*{}\"", script.to_string_lossy().replace('\\', "/"));
                let new = if old.contains("[autoload]") { old.replacen("[autoload]", &format!("[autoload]\n{line}"), 1) } else { format!("{old}\n[autoload]\n{line}\n") };
                std::fs::write(&cfg, new).map_err(|e| format!("couldn't add HotSwap to the game: {e}"))?;
            }
            Ok(())
        }
        // Script Hook V loads every .asi file in the game folder.
        "gta" => {
            if !has_script_hook(game_dir) {
                return Err("Script Hook V isn't installed yet: follow the setup steps on this page.".into());
            }
            let p = game_dir.join("HotSwap.asi");
            if std::fs::read(&p).ok().as_deref() != Some(GTA_ASI) {
                std::fs::write(&p, GTA_ASI).map_err(|e| format!("couldn't add HotSwap to the game: {e}"))?;
            }
            Ok(())
        }
        _ => Err("not a script engine".into()),
    }
}

fn find_pck(dir: &Path) -> Option<PathBuf> {
    let mut dirs = vec![dir.to_path_buf()];
    dirs.extend(std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
    dirs.into_iter().find_map(|d| std::fs::read_dir(d).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pck"))))
}

/// The .pck format version: 1 for Godot 3, 2 and up for Godot 4.
fn godot_major(pck: &Path) -> Option<u32> {
    use std::io::Read;
    let mut b = [0u8; 8];
    std::fs::File::open(pck).ok()?.read_exact(&mut b).ok()?;
    (&b[..4] == b"GDPC").then(|| u32::from_le_bytes(b[4..8].try_into().unwrap()))
}

/// The Godot script for version 4, or rewritten for Godot 3's names.
fn godot_script(v4: bool) -> String {
    if v4 {
        return GODOT.to_string();
    }
    GODOT.replace("JSON.parse_string(", "_parse(")
        .replace("JSON.stringify(", "to_json(")
        .replace(".to_utf8_buffer()", ".to_utf8()")
        .replace("Time.get_ticks_msec()", "OS.get_ticks_msec()")
        .replace("process_mode = Node.PROCESS_MODE_ALWAYS", "pause_mode = Node.PAUSE_MODE_PROCESS")
        .replace(".find_child(", ".find_node(")
        + "\nfunc _parse(s):\n\tvar r = JSON.parse(s)\n\treturn r.result if r.error == OK else null\n"
}

pub struct Session {
    engine: String,
    pid: Option<u32>,
    /// When the game's script last asked.
    seen: Option<Instant>,
    /// Changes every time HotSwap connects, so the script starts its action counts over.
    run: u64,
    pressed: HashMap<String, u64>,
}

impl Session {
    pub fn attach(engine: &str, dir: Option<PathBuf>, pid: Option<u32>) -> Result<Session, String> {
        socket().ok_or("couldn't open HotSwap's local port")?;
        dir.ok_or_else(|| "couldn't find the game's folder".to_string()).and_then(|d| install(engine, &d))?;
        let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(1);
        Ok(Session { engine: engine.to_string(), pid, seen: None, run, pressed: HashMap::new() })
    }

    pub fn tick(&mut self, specs: &HashMap<String, Vec<Spec>>, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Result<bool, String> {
        for a in actions {
            *self.pressed.entry(a.clone()).or_default() += 1;
        }
        let mods: Vec<Value> = specs.iter().filter(|(_, l)| !l.iter().any(|s| s.act)).map(|(id, l)| {
            let off = l.first().map_or(0.0, |s| if matches!(s.op.as_str(), "scale" | "value") { s.def } else { 0.0 });
            json!({ "id": id, "specs": l, "value": values.and_then(|v| v.get(id)).copied().unwrap_or(off) })
        }).collect();
        let acts: Vec<Value> = self.pressed.iter().filter_map(|(id, n)| Some(json!({ "id": id, "n": n, "specs": specs.get(id)? }))).collect();
        let reply = json!({ "run": self.run, "mods": mods, "actions": acts }).to_string();
        let Some(s) = socket() else { return Err("couldn't open HotSwap's local port".into()) };
        // GTA with BattlEye on: answer nothing (the plugin then keeps every mod off).
        if self.engine == "gta" && battleye(self.pid) {
            let mut buf = [0u8; 512];
            while s.recv_from(&mut buf).is_ok() {}
            return Err("BattlEye is on, so mods are off. Follow step 3 on this page (start GTA with -nobattleye) for story mode mods.".into());
        }
        let mut buf = [0u8; 512];
        while let Ok((n, from)) = s.recv_from(&mut buf) {
            if from.ip().is_loopback() && buf[..n].starts_with(b"hs") {
                self.seen = Some(Instant::now());
                let _ = s.send_to(reply.as_bytes(), from);
            }
        }
        if self.seen.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
            return Err(if self.engine == "gta" {
                "Waiting for story mode. Just installed Script Hook V? Restart GTA once so it loads HotSwap.".into()
            } else {
                "HotSwap is added to the game. Restart the game once so it loads HotSwap.".into()
            });
        }
        Ok(true)
    }

    /// The script puts things back by itself when HotSwap stops answering.
    pub fn restore(&mut self, specs: &HashMap<String, Vec<Spec>>) {
        let _ = self.tick(specs, None, &[]);
    }
}
