//! The app's side of the mod engine (engine.rs, hotswap-engine.exe next to hotswap.exe): starts it
//! when a game with mods is chosen, sends it what the interface sets, passes its events on to the
//! interface, and starts it again if it stops. If the engine can't start (say a security tool
//! removed it), the game's status line says so and everything else keeps working.

use crate::engines::Spec;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

/// Events the engine may pass to the interface.
const EVENTS: [&str; 2] = ["trainer-status", "radar"];
pub const MISSING: &str = "HotSwap's mod engine (hotswap-engine.exe) is missing. Windows Security may have removed it: see Settings → About.";

#[derive(Default)]
struct Inner {
    app: OnceLock<AppHandle>,
    /// The running engine: its input, the process, and which start it was.
    child: Mutex<Option<(ChildStdin, Child, u64)>>,
    starts: AtomicU64,
    last_try: Mutex<Option<Instant>>,
    /// The last "set", sent again to a restarted engine.
    last_set: Mutex<Option<Value>>,
    game: Mutex<Option<String>>,
    pos: Mutex<Option<[f64; 4]>>,
    replies: Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>,
    next_id: AtomicU64,
}

#[derive(Clone, Default)]
pub struct Engine(Arc<Inner>);

pub fn exe() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.with_file_name("hotswap-engine.exe"))
}

impl Engine {
    pub fn start(&self, app: AppHandle) {
        let _ = self.0.app.set(app);
        // Starts the engine again if it stopped while a game with mods is chosen.
        let me = self.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(2));
            let wanted = me.0.game.lock().unwrap().is_some();
            if wanted && me.0.child.lock().unwrap().is_none() {
                me.ensure();
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set(&self, game: Option<String>, paused: bool, values: HashMap<String, f64>, engine: Option<String>, specs: HashMap<String, Vec<Spec>>, guard: Vec<Spec>, install: Option<PathBuf>) {
        let msg = json!({ "t": "set", "game": game, "paused": paused, "values": values, "engine": engine, "specs": specs, "guard": guard, "install": install });
        *self.0.last_set.lock().unwrap() = Some(msg.clone());
        *self.0.game.lock().unwrap() = game.clone();
        if game.is_some() {
            self.ensure();
        }
        // No game: only tell a running engine (it puts back what it changed), never start one for it.
        self.send(&msg);
    }

    pub fn action(&self, id: String) {
        self.send(&json!({ "t": "action", "id": id }));
    }

    pub fn teleport(&self, to: [f64; 3]) {
        self.send(&json!({ "t": "teleport", "to": to }));
    }

    pub fn position(&self) -> Option<[f64; 4]> {
        *self.0.pos.lock().unwrap()
    }

    /// What the map draws, read from the game by the engine.
    pub fn world(&self, pid: u32) -> Result<Value, String> {
        self.ensure();
        let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.0.replies.lock().unwrap().insert(id, tx);
        if !self.send(&json!({ "t": "world", "id": id, "pid": pid })) {
            self.0.replies.lock().unwrap().remove(&id);
            return Err("HotSwap's mod engine isn't running".into());
        }
        let r = rx.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| Err("The game took too long to answer".into()));
        self.0.replies.lock().unwrap().remove(&id);
        r
    }

    fn send(&self, msg: &Value) -> bool {
        let mut child = self.0.child.lock().unwrap();
        let Some((stdin, _, _)) = child.as_mut() else { return false };
        if writeln!(stdin, "{msg}").and_then(|_| stdin.flush()).is_ok() {
            return true;
        }
        // Gone: the reader notices too; the supervisor starts it again.
        *child = None;
        false
    }

    fn status(&self, message: &str) {
        if let Some(app) = self.0.app.get() {
            let game = self.0.game.lock().unwrap().clone();
            let _ = app.emit("trainer-status", json!({ "game": game, "connected": false, "message": message }));
        }
    }

    /// Starts the engine unless it runs, at most every few seconds.
    fn ensure(&self) {
        if self.0.child.lock().unwrap().is_some() {
            return;
        }
        {
            let mut last = self.0.last_try.lock().unwrap();
            if last.is_some_and(|t| t.elapsed() < Duration::from_secs(5)) {
                return;
            }
            *last = Some(Instant::now());
        }
        let path = exe().filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() > 0));
        let Some(path) = path else {
            crate::logs::write("engine", "hotswap-engine.exe is missing");
            return self.status(MISSING);
        };
        let mut cmd = Command::new(&path);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                crate::logs::write("engine", &format!("couldn't start hotswap-engine.exe: {e}"));
                return self.status(&format!("HotSwap's mod engine couldn't start ({e}). Windows Security may have blocked it: see Settings → About."));
            }
        };
        let n = self.0.starts.fetch_add(1, Ordering::Relaxed) + 1;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else { return };
        crate::logs::write("engine", &format!("started hotswap-engine.exe (pid {})", child.id()));
        *self.0.child.lock().unwrap() = Some((stdin, child, n));
        let me = self.clone();
        std::thread::spawn(move || me.read(stdout, n));
        // A new engine knows nothing yet: the current game and mods first.
        if let Some(set) = self.0.last_set.lock().unwrap().clone() {
            self.send(&set);
        }
    }

    fn read(&self, stdout: std::process::ChildStdout, n: u64) {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            match v["t"].as_str() {
                Some("ev") => {
                    let name = v["name"].as_str().unwrap_or("");
                    if let (true, Some(app)) = (EVENTS.contains(&name), self.0.app.get()) {
                        let _ = app.emit(name, &v["data"]);
                    }
                }
                Some("pos") => *self.0.pos.lock().unwrap() = serde_json::from_value(v["pos"].clone()).ok().flatten(),
                Some("reply") => {
                    let id = v["id"].as_u64().unwrap_or(u64::MAX);
                    if let Some(tx) = self.0.replies.lock().unwrap().remove(&id) {
                        let r = if v.get("ok").is_some() { Ok(v["ok"].clone()) } else { Err(v["err"].as_str().unwrap_or("The mod engine failed").to_string()) };
                        let _ = tx.send(r);
                    }
                }
                _ => {}
            }
        }
        // It stopped: forget it (unless a newer one already took over) and say so.
        let mut child = self.0.child.lock().unwrap();
        if child.as_ref().is_some_and(|c| c.2 == n) {
            if let Some((_, mut c, _)) = child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            drop(child);
            *self.0.pos.lock().unwrap() = None;
            crate::logs::write("engine", "hotswap-engine.exe stopped");
            if self.0.game.lock().unwrap().is_some() {
                let gone = exe().is_none_or(|p| !p.exists());
                self.status(if gone { MISSING } else { "HotSwap's mod engine stopped. Starting it again…" });
            }
        }
    }
}
