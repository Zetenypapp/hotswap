//! Scrap Mechanic (Steam app 387990). Its game logic is Lua scripts in the install, so instead of
//! reading memory, Hotswap adds a small bridge (assets/sm_bridge.lua) to a few survival scripts
//! while the game runs and talks to it through JSON files in Survival/hotswap:
//! - in_<n>.json: the mods, a new numbered file per change (written here, read by the bridge)
//! - out.json: your position and health, about once a second (written by the bridge)
//! - map_<seed>.json: the world's colors and heights, made once per world for the map
//! The added lines and the bridge are removed again when the game closes.

use crate::{mapimage, trainer::log};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub const SCRAP: &str = "387990";

const BRIDGE: &str = include_str!("../assets/sm_bridge.lua");
const BRIDGE_FILE: &str = r"Scripts\hotswap_bridge.lua";
const MARK: &str = "-- Hotswap: added while the game runs, removed when it closes";
const HOOK: &str = r#"if not Hotswap then pcall( dofile, "$SURVIVAL_DATA/Scripts/hotswap_bridge.lua" ) end if Hotswap then pcall( Hotswap.hook ) end"#;
/// The scripts that load the bridge (it wraps functions from each).
const HOOKED: [&str; 5] = [
    r"Scripts\game\SurvivalGame.lua",
    r"Scripts\game\SurvivalPlayer.lua",
    r"Scripts\game\characters\MechanicCharacter.lua",
    r"Scripts\game\interactables\Crafter.lua",
    r"Scripts\terrain\terrain_overworld.lua",
];

pub struct Session {
    survival: PathBuf,
    seq: u64,
    last: String,
    /// Times each action was pressed this game run.
    actions: HashMap<String, u64>,
    tp: Option<(u64, [f64; 2])>,
    /// Map position: [x, y (south), height, yaw clockwise from east].
    pub pos: Option<[f64; 4]>,
}

/// The Survival folder of the install the game at `pid` runs from.
fn survival_of(pid: u32) -> Option<PathBuf> {
    let mut sys = sysinfo::System::new();
    let p = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[p]),
        true,
        sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always),
    );
    let exe = sys.process(p)?.exe()?.to_path_buf();
    // <install>\Release\ScrapMechanic.exe
    Some(exe.parent()?.parent()?.join("Survival")).filter(|d| d.is_dir())
}

/// Adds the bridge to the game's scripts (idempotent) and starts a fresh settings folder.
pub fn install(survival: &Path) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    let dir = survival.join("hotswap");
    fs::create_dir_all(&dir).map_err(io)?;
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("in_") || name == "out.json" {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    fs::write(dir.join("map_wanted.json"), "{}").map_err(io)?;
    fs::write(survival.join(BRIDGE_FILE), BRIDGE).map_err(io)?;
    for f in HOOKED {
        let path = survival.join(f);
        let s = fs::read_to_string(&path).map_err(|e| format!("{f}: {e}"))?;
        if !s.contains(MARK) {
            fs::write(&path, format!("{s}\n{MARK}\n{HOOK}\n")).map_err(io)?;
        }
    }
    Ok(())
}

/// Takes the bridge out again, leaving the game's scripts exactly as they were.
pub fn uninstall(survival: &Path) {
    for f in HOOKED {
        let path = survival.join(f);
        let Ok(s) = fs::read_to_string(&path) else { continue };
        if let Some(i) = s.find(MARK) {
            let mut orig = s[..i].to_string();
            if orig.ends_with('\n') {
                orig.pop();
            }
            let _ = fs::write(&path, orig);
        }
    }
    let _ = fs::remove_file(survival.join(BRIDGE_FILE));
    if let Ok(entries) = fs::read_dir(survival.join("hotswap")) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("in_") || name == "out.json" || name == "map_wanted.json" {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

impl Session {
    pub fn attach(pid: u32) -> Result<Session, String> {
        let survival = survival_of(pid).ok_or("Couldn't find the game's Survival folder")?;
        install(&survival)?;
        log(&format!("scrap: bridge installed in {}", survival.display()));
        Ok(Session { survival, seq: 0, last: String::new(), actions: HashMap::new(), tp: None, pos: None })
    }

    /// Sends the mods to the bridge when they change, and reads back where you are. Ok(true) once
    /// the bridge is running in a loaded world.
    pub fn tick(&mut self, values: Option<&HashMap<String, f64>>, actions: &[String], teleport: Option<[f64; 3]>) -> Result<bool, String> {
        for a in actions {
            *self.actions.entry(a.clone()).or_default() += 1;
        }
        if let Some([x, y, _]) = teleport {
            // Map y points south; the game's y points north.
            let n = self.tp.map_or(0, |(n, _)| n) + 1;
            self.tp = Some((n, [x, -y]));
        }
        let mut msg = serde_json::json!({ "mods": values.cloned().unwrap_or_default(), "actions": self.actions });
        if let Some((n, [x, y])) = self.tp {
            msg["tp"] = serde_json::json!({ "n": n, "x": x, "y": y });
        }
        let text = msg.to_string();
        if text != self.last {
            let dir = self.survival.join("hotswap");
            let tmp = dir.join("in.tmp");
            fs::write(&tmp, &text).and_then(|_| fs::rename(&tmp, dir.join(format!("in_{}.json", self.seq + 1)))).map_err(|e| e.to_string())?;
            self.seq += 1;
            self.last = text;
        }
        // The bridge writes out.json about once a second while a world is open.
        let out = self.survival.join(r"hotswap\out.json");
        let fresh = fs::metadata(&out)
            .and_then(|m| m.modified())
            .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() < Duration::from_secs(4));
        if !fresh {
            self.pos = None;
            return Ok(false);
        }
        let v: serde_json::Value = fs::read_to_string(&out).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let n = |k: &str| v.get(k).and_then(|x| x.as_f64());
        self.pos = match (n("x"), n("y"), n("z"), n("dx"), n("dy")) {
            (Some(x), Some(y), Some(z), Some(dx), Some(dy)) => Some([x, -y, z, (-dy).atan2(dx).to_degrees()]),
            _ => None,
        };
        Ok(true)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        uninstall(&self.survival);
        log("scrap: bridge removed");
    }
}

/// The map covers this square (meters, game axes): the overworld is 8192 x 6144 m, centered.
pub const MAP_HALF: f64 = 4096.0;
const SIZE: usize = 2048; // 4 m per pixel

/// Builds the map images in `dir` from the newest world the bridge saved, unless already done.
pub fn ensure_map(install: &Path, dir: &Path) -> Result<(), String> {
    let saved = install.join(r"Survival\hotswap");
    let newest = fs::read_dir(&saved)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("map_") && e.file_name().to_string_lossy().ends_with(".json") && e.file_name() != "map_wanted.json")
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max();
    let Some((when, file)) = newest else {
        return Err("The map is made the first time you load a survival world while Hotswap is open. Load your world, then come back here.".into());
    };
    let stamp = format!("sm-v1 {} {:?}", file.display(), when);
    let stamp_file = dir.join("stamp.txt");
    if fs::read_to_string(&stamp_file).is_ok_and(|s| s == stamp) && dir.join(mapimage::OVERVIEW).exists() {
        return Ok(());
    }
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let rgb = render(&v)?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    mapimage::write_set(dir, &rgb, SIZE)?;
    fs::write(stamp_file, stamp).map_err(|e| e.to_string())
}

/// Colors with hill shading, north up, on open sea outside the world.
fn render(v: &serde_json::Value) -> Result<Vec<u8>, String> {
    let rows = |k: &str| -> Vec<Vec<u8>> {
        v[k].as_array().map(|a| a.iter().map(|r| r.as_str().unwrap_or("").as_bytes().to_vec()).collect()).unwrap_or_default()
    };
    let (colors, heights) = (rows("colors"), rows("heights"));
    let num = |k: &str| v[k].as_f64().unwrap_or(0.0);
    let (x0, y1, step) = (num("x0"), num("y1"), num("step"));
    if colors.is_empty() || step <= 0.0 {
        return Err("The saved map is empty".into());
    }
    let hex = |b: &[u8]| -> u32 { b.iter().fold(0, |a, c| a * 16 + (*c as char).to_digit(16).unwrap_or(0)) };
    let (hw, hh) = (heights.first().map_or(0, |r| r.len() / 3), heights.len());
    let height = |x: usize, y: usize| -> f64 {
        let (x, y) = (x.min(hw.saturating_sub(1)), y.min(hh.saturating_sub(1)));
        heights.get(y).and_then(|r| r.get(x * 3..x * 3 + 3)).map_or(0.0, |b| hex(b) as f64 / 4.0 - 64.0)
    };
    let mut rgb = vec![0u8; SIZE * SIZE * 3];
    let px_m = MAP_HALF * 2.0 / SIZE as f64;
    for py in 0..SIZE {
        for px in 0..SIZE {
            // Game position of this pixel, then the saved sample there.
            let gx = -MAP_HALF + (px as f64 + 0.5) * px_m;
            let gy = MAP_HALF - (py as f64 + 0.5) * px_m;
            let (cx, cy) = (((gx - x0) / step).floor(), ((y1 - gy) / step).floor());
            let o = (py * SIZE + px) * 3;
            let row = (cy >= 0.0).then(|| colors.get(cy as usize)).flatten();
            let Some(c) = row.and_then(|r| (cx >= 0.0).then(|| r.get(cx as usize * 6..cx as usize * 6 + 6)).flatten()) else {
                rgb[o..o + 3].copy_from_slice(&[24, 70, 112]);
                continue;
            };
            // Light from the north-west: slopes facing it are brighter.
            let (hx, hy) = (cx as usize / 2, cy as usize / 2);
            let dzdx = (height(hx + 1, hy) - height(hx.saturating_sub(1), hy)) / (4.0 * step);
            let dzdy = (height(hx, hy + 1) - height(hx, hy.saturating_sub(1))) / (4.0 * step);
            let shade = (1.0 + 0.9 * (dzdx + dzdy)).clamp(0.55, 1.35);
            for i in 0..3 {
                rgb[o + i] = (hex(&c[i * 2..i * 2 + 2]) as f64 * shade).clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok(rgb)
}
