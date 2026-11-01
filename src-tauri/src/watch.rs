//! Detects which installed game is running: Steam's RunningAppID first, then any
//! process whose executable lives inside a game's install folder. Emits
//! `game-running` with the game ID (or null) whenever that changes.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Default)]
pub struct Watch {
    /// (game ID, lowercased install folder ending in a backslash)
    targets: Arc<Mutex<Vec<(String, String)>>>,
    running: Arc<Mutex<Option<String>>>,
}

impl Watch {
    pub fn set_targets(&self, targets: Vec<(String, PathBuf)>) {
        let list = targets
            .into_iter()
            .map(|(id, dir)| {
                let mut d = dir.to_string_lossy().to_lowercase();
                if !d.ends_with('\\') {
                    d.push('\\');
                }
                (id, d)
            })
            .collect();
        *self.targets.lock().unwrap() = list;
    }

    /// Install folder of a scanned game.
    pub fn install_of(&self, id: &str) -> Option<PathBuf> {
        self.targets.lock().unwrap().iter().find(|t| t.0 == id).map(|t| PathBuf::from(&t.1))
    }

    pub fn running(&self) -> Option<String> {
        self.running.lock().unwrap().clone()
    }

    pub fn start(&self, app: AppHandle) {
        let me = self.clone();
        thread::spawn(move || {
            let mut sys = System::new();
            loop {
                let now = me.detect(&mut sys);
                let changed = {
                    let mut cur = me.running.lock().unwrap();
                    let changed = *cur != now;
                    *cur = now.clone();
                    changed
                };
                if changed {
                    let _ = app.emit("game-running", &now);
                    // Game Boost starts or ends on its own thread (it waits for the game's process).
                    let (app, me, now) = (app.clone(), me.clone(), now.clone());
                    thread::spawn(move || app.state::<crate::boost::Boost>().game_changed(&app, now, |id| me.pids_in(id)));
                }
                thread::sleep(Duration::from_secs(2));
            }
        });
    }

    /// Process ID of the game's main Unreal executable (`*-Win64-Shipping.exe` in its folder).
    pub fn pid_of(&self, id: &str) -> Option<u32> {
        let (_, dir) = self.targets.lock().unwrap().iter().find(|(gid, _)| gid == id).cloned()?;
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
        // Unreal games run a "-Win64-Shipping" exe (the launcher exe is only a stub); Unity games
        // run their main exe, the one with a "<name>_Data" folder next to it; a few others by name.
        let exes: Vec<(u32, String)> = sys.processes().values()
            .filter_map(|p| Some((p.pid().as_u32(), p.exe()?.to_string_lossy().to_lowercase())))
            .filter(|(_, e)| e.starts_with(&dir) && e.ends_with(".exe"))
            .collect();
        exes.iter().find(|(_, e)| e.ends_with("-win64-shipping.exe"))
            .or_else(|| exes.iter().find(|(_, e)| std::path::Path::new(&format!("{}_Data", e.trim_end_matches(".exe"))).is_dir()))
            // Games on their own engine, by exe name.
            .or_else(|| exes.iter().find(|(_, e)| [r"\scrapmechanic.exe", r"\stormworks64.exe", r"\stormworks.exe", r"\eurotrucks2.exe", r"\amtrucks.exe", r"\gta5.exe", r"\gta5_enhanced.exe"].iter().any(|n| e.ends_with(n))))
            .map(|(pid, _)| *pid)
    }

    /// Every process running from the game's install folder (games can have several).
    pub fn pids_in(&self, id: &str) -> Vec<u32> {
        let Some((_, dir)) = self.targets.lock().unwrap().iter().find(|(gid, _)| gid == id).cloned() else { return vec![] };
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
        sys.processes().values()
            .filter(|p| p.exe().is_some_and(|e| e.to_string_lossy().to_lowercase().starts_with(&dir)))
            .map(|p| p.pid().as_u32())
            .collect()
    }

    /// Ends every process running from the game's install folder. Returns how many were ended.
    pub fn kill(&self, id: &str) -> usize {
        let Some((_, dir)) = self.targets.lock().unwrap().iter().find(|(gid, _)| gid == id).cloned() else { return 0 };
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
        sys.processes()
            .values()
            .filter(|p| p.exe().is_some_and(|e| e.to_string_lossy().to_lowercase().starts_with(&dir)))
            .filter(|p| p.kill())
            .count()
    }

    fn detect(&self, sys: &mut System) -> Option<String> {
        let targets = self.targets.lock().unwrap().clone();
        if targets.is_empty() {
            return None;
        }
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet).with_cmd(UpdateKind::OnlyIfNotSet),
        );
        // Helper processes don't make a game "running": Chromium/Electron child processes (they can
        // outlive a crashed game) and crash reporters.
        let is_game = |p: &sysinfo::Process| {
            !p.cmd().iter().any(|a| a.to_string_lossy().starts_with("--type="))
                && !p.name().to_string_lossy().to_lowercase().contains("crashpad")
        };
        // Steam's word first, but only while something from the game's folder really runs: Steam
        // keeps reporting a game for a while after it crashed or was ended.
        if let Some(id) = steam_running_app() {
            if let Some((_, dir)) = targets.iter().find(|(gid, _)| *gid == id) {
                if sys.processes().values().any(|p| is_game(p) && p.exe().is_some_and(|e| e.to_string_lossy().to_lowercase().starts_with(dir.as_str()))) {
                    return Some(id);
                }
            }
        }
        for process in sys.processes().values().filter(|p| is_game(p)) {
            let Some(exe) = process.exe() else { continue };
            let exe = exe.to_string_lossy().to_lowercase();
            if let Some((id, _)) = targets.iter().find(|(_, dir)| exe.starts_with(dir.as_str())) {
                return Some(id.clone());
            }
        }
        None
    }
}

fn steam_running_app() -> Option<String> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam").ok()?;
        let id: u32 = key.get_value("RunningAppID").ok()?;
        return (id != 0).then(|| id.to_string());
    }
    #[allow(unreachable_code)]
    None
}
