//! The app's side of the mod engine (engine.rs, hotswap-engine.exe, its own download since 0.5.0): starts it
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

/// The engine: in release builds the downloaded one (engine_dir), in dev builds the one cargo builds
/// next to the app.
pub fn exe() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        let me = std::env::current_exe().ok()?;
        return [me.with_file_name("hotswap-engine.exe"), me.with_file_name("hsengine.exe")].into_iter().find(|p| p.is_file());
    }
    Some(engine_dir()?.join("hotswap-engine.exe"))
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
        let Some(app) = self.0.app.get() else { return };
        if let Some(problem) = engine_problem(app) {
            crate::logs::write("engine", problem);
            return self.status(problem);
        }
        let Some(path) = exe() else { return };
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
                let problem = self.0.app.get().and_then(engine_problem);
                self.status(problem.unwrap_or("HotSwap's mod engine stopped. Starting it again…"));
            }
        }
    }
}

/* ---------- The engine as its own download (0.5.0) ----------
   The installer carries no game-memory code: people who want mods get hotswap-engine.exe from the
   release that matches their HotSwap, checked against HotSwap's update key before it's saved. */

const RELEASES: &str = "https://github.com/Zetenypapp/hotswap/releases/download";
const MAX_ENGINE: u64 = 64 << 20;
pub const NOT_INSTALLED: &str = "Mods need HotSwap's mod engine, a separate download. Get it on this page or in Settings → Features.";
const UPDATING: &str = "HotSwap's mod engine is updating to match this version of HotSwap…";

/// Where the downloaded engine lives: HotSwap's local data folder (%LOCALAPPDATA%\<identifier>\engine).
fn engine_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("app.hotswap.desktop").join("engine"))
}

/// The engine's version, written next to it when it's installed.
fn installed_version() -> Option<String> {
    std::fs::read_to_string(engine_dir()?.join("version.txt")).ok().map(|v| v.trim().to_string())
}

fn wanted_version(app: &AppHandle) -> String {
    crate::app::shown_version(&app.package_info().version.to_string())
}

/// Why the engine can't run right now, if it can't. Dev builds use the one cargo built.
fn engine_problem(app: &AppHandle) -> Option<&'static str> {
    if exe().is_some_and(|p| std::fs::metadata(p).is_ok_and(|m| m.len() > 0)) {
        return (!cfg!(debug_assertions) && installed_version().as_deref() != Some(wanted_version(app).as_str())).then_some(UPDATING);
    }
    Some(if cfg!(debug_assertions) || engine_dir().is_some_and(|d| d.join("version.txt").exists()) { MISSING } else { NOT_INSTALLED })
}

/// Downloads `hotswap-engine.exe` and its signature from `base`, and returns the exe once the
/// signature matches HotSwap's update key.
fn fetch_verified(base: &str, key_b64: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let get = |url: &str, max: u64| -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        ureq::get(url).timeout(Duration::from_secs(180)).call().map_err(|e| format!("couldn't download it ({e})"))?
            .into_reader().take(max).read_to_end(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    };
    let data = get(&format!("{base}/hotswap-engine.exe"), MAX_ENGINE + 1)?;
    if data.len() as u64 > MAX_ENGINE || !data.starts_with(b"MZ") {
        return Err("the download isn't the mod engine".into());
    }
    let sig = String::from_utf8(get(&format!("{base}/hotswap-engine.exe.sig"), 64 << 10)?).map_err(|_| "bad signature file")?;
    crate::catalog::verify_with(key_b64, &data, &sig)?;
    Ok(data)
}

impl Engine {
    /// Stops the running engine (to replace or remove it). The 2-second loop starts it again when needed.
    fn stop(&self) {
        if let Some((_, mut c, _)) = self.0.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        *self.0.last_try.lock().unwrap() = None;
    }

    /// Downloads the engine for this version of HotSwap and puts it in place.
    fn install(&self, app: &AppHandle) -> Result<(), String> {
        if cfg!(debug_assertions) {
            return Err("Dev builds use the engine cargo builds".into());
        }
        let data = fetch_verified(&format!("{RELEASES}/v{}", wanted_version(app)), &crate::catalog::key_of(app))?;
        let dir = engine_dir().ok_or("No local data folder")?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let part = dir.join("hotswap-engine.part");
        std::fs::write(&part, &data).map_err(|e| e.to_string())?;
        self.stop();
        let exe = dir.join("hotswap-engine.exe");
        let _ = std::fs::remove_file(&exe);
        std::fs::rename(&part, &exe).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("version.txt"), wanted_version(app)).map_err(|e| e.to_string())?;
        crate::logs::write("engine", &format!("installed the mod engine {} ({} KB)", wanted_version(app), data.len() / 1024));
        Ok(())
    }

    /// At start: a 0.4.x install had the engine next to the app, so mods keep working with a fresh
    /// one; anyone who has the engine gets the one matching this version after an update.
    pub fn startup(&self, app: &AppHandle) {
        if cfg!(debug_assertions) {
            return;
        }
        let old = std::env::current_exe().ok().map(|e| e.with_file_name("hotswap-engine.exe")).filter(|o| o.exists());
        if let Some(o) = &old {
            let _ = std::fs::remove_file(o);
        }
        let had = old.is_some() || engine_dir().is_some_and(|d| d.exists());
        if had && engine_problem(app).is_some() {
            let (me, app) = (self.clone(), app.clone());
            std::thread::spawn(move || {
                let r = me.install(&app);
                if let Err(e) = &r {
                    crate::logs::write("engine", &format!("couldn't update the mod engine: {e}"));
                }
                let _ = app.emit("engine", engine_state_of(&app));
            });
        }
    }
}

fn engine_state_of(app: &AppHandle) -> Value {
    let problem = engine_problem(app);
    json!({
        "installed": problem.is_none() || problem == Some(UPDATING),
        "ready": problem.is_none(),
        "dev": cfg!(debug_assertions),
        "version": installed_version(),
        "message": problem,
    })
}

#[tauri::command]
pub fn engine_state(app: AppHandle) -> Value {
    engine_state_of(&app)
}

#[tauri::command]
pub async fn engine_install(app: AppHandle, engine: tauri::State<'_, Engine>) -> Result<Value, String> {
    let engine = engine.inner().clone();
    let a = app.clone();
    tauri::async_runtime::spawn_blocking(move || engine.install(&a)).await.map_err(|e| e.to_string())??;
    Ok(engine_state_of(&app))
}

#[tauri::command]
pub fn engine_remove(app: AppHandle, engine: tauri::State<'_, Engine>) -> Result<Value, String> {
    if cfg!(debug_assertions) {
        return Err("Dev builds use the engine cargo builds".into());
    }
    engine.stop();
    if let Some(d) = engine_dir().filter(|d| d.exists()) {
        std::fs::remove_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::logs::write("engine", "removed the mod engine");
    Ok(engine_state_of(&app))
}

#[cfg(test)]
mod tests {
    /// End to end against a local server: `HS_ENGINE_TEST=http://127.0.0.1:8765 cargo test --lib engine_download -- --ignored`
    /// with hotswap-engine.exe and its .sig (signed with HotSwap's key) served there.
    #[test]
    #[ignore]
    fn engine_download() {
        let conf: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("tauri.conf.json").unwrap()).unwrap();
        let key = conf["plugins"]["updater"]["pubkey"].as_str().unwrap();
        let base = std::env::var("HS_ENGINE_TEST").unwrap();
        let data = super::fetch_verified(&base, key).expect("signed engine");
        assert!(data.starts_with(b"MZ"));
        // A file whose signature doesn't match is refused.
        assert!(super::fetch_verified(&format!("{base}/bad"), key).is_err());
    }
}
