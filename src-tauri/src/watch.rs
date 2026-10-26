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
use tauri::{AppHandle, Emitter};

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
                }
                thread::sleep(Duration::from_secs(2));
            }
        });
    }

    fn detect(&self, sys: &mut System) -> Option<String> {
        let targets = self.targets.lock().unwrap().clone();
        if targets.is_empty() {
            return None;
        }
        if let Some(id) = steam_running_app() {
            if targets.iter().any(|(gid, _)| *gid == id) {
                return Some(id);
            }
        }
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
        );
        for process in sys.processes().values() {
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
