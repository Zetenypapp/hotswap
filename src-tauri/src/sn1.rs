//! Subnautica (Steam app 264710), a Unity game on Mono. Classes and fields are looked up by name
//! through Mono (see mono.rs). Most mods switch the game's own cheats: the GameModeUtils cheat
//! flags and the console cheat objects (NoDamageConsoleCommand, NoCostConsoleCommand), so the game
//! handles them exactly as it does for its console commands.

use crate::{mono::Mono, trainer::log};
use std::collections::HashMap;

pub const SUBNAUTICA: &str = "264710";

/// Cheat flag mods: mod ID -> GameModeOption bit.
const FLAGS: [(&str, u32); 6] = [
    ("infinite-oxygen", 64),        // NoOxygen
    ("free-crafting", 4),           // NoCost
    ("unlock-all-blueprints", 8),   // NoBlueprints
    ("infinite-energy", 16),        // NoEnergy
    ("no-crush-depth", 32),         // NoPressure
    ("passive-creatures", 128),     // NoAggression
];

#[derive(Default, Clone, Copy)]
struct Targets {
    player: u64,
    cheats: u64,
    /// GameModeUtils.currentGameMode: Creative and Freedom include some of the cheat flags.
    mode: u64,
    no_damage: u64,
    no_cost: u64,
    live: u64,
    health: u64,
    max_health: u64,
    survival: u64,
    swim_motor: u64,
    walk_motor: u64,
    controller: u64,
    oxygen_mgr: u64,
}

/// Player speed limits: (PlayerController default, motor field it's copied into, which slider).
const SPEEDS: [(&str, &str, &str); 13] = [
    ("swimWaterAcceleration", "waterAcceleration", "swim"),
    ("seaglideWaterAcceleration", "waterAcceleration", "glide"),
    ("swimForwardMaxSpeed", "forwardMaxSpeed", "swim"),
    ("swimBackwardMaxSpeed", "backwardMaxSpeed", "swim"),
    ("swimStrafeMaxSpeed", "strafeMaxSpeed", "swim"),
    ("swimVerticalMaxSpeed", "verticalMaxSpeed", "swim"),
    ("seaglideForwardMaxSpeed", "forwardMaxSpeed", "glide"),
    ("seaglideBackwardMaxSpeed", "backwardMaxSpeed", "glide"),
    ("seaglideStrafeMaxSpeed", "strafeMaxSpeed", "glide"),
    ("seaglideVerticalMaxSpeed", "verticalMaxSpeed", "glide"),
    ("walkRunForwardMaxSpeed", "forwardMaxSpeed", "walk"),
    ("walkRunBackwardMaxSpeed", "backwardMaxSpeed", "walk"),
    ("walkRunStrafeMaxSpeed", "strafeMaxSpeed", "walk"),
];

pub struct Session {
    m: Mono,
    t: Targets,
    off: HashMap<&'static str, u64>,
    /// Console cheat switches we turned on: address -> the value before.
    switched: HashMap<u64, u8>,
    /// The game's own speed limits (PlayerController field -> value), read once per player.
    speed_orig: HashMap<&'static str, f32>,
    /// Speed factors last applied: (swim, walk).
    speed_last: (f32, f32),
}

impl Session {
    pub fn new(m: Mono) -> Session {
        Session { m, t: Targets::default(), off: HashMap::new(), switched: HashMap::new(), speed_orig: HashMap::new(), speed_last: (1.0, 1.0) }
    }

    /// Offset of `class.field`, cached.
    fn f(&mut self, class: &str, field: &'static str) -> Option<u64> {
        if let Some(&o) = self.off.get(field) {
            return Some(o);
        }
        let c = self.m.class("", class)?;
        let o = self.m.field(c, field)? as u64;
        self.off.insert(field, o);
        Some(o)
    }
    fn ptr_at(&mut self, obj: u64, class: &str, field: &'static str) -> Option<u64> {
        let o = self.f(class, field)?;
        self.m.p.ptr(obj + o)
    }
    fn static_obj(&mut self, class: &str, field: &str) -> Option<u64> {
        let c = self.m.class("", class)?;
        let a = self.m.static_addr(c, field)?;
        self.m.p.ptr(a)
    }

    fn resolve(&mut self, player: u64) {
        let mut t = Targets { player, ..Default::default() };
        if let Some(c) = self.m.class("", "GameModeUtils") {
            t.cheats = self.m.static_addr(c, "currentCheats").unwrap_or(0);
            t.mode = self.m.static_addr(c, "currentGameMode").unwrap_or(0);
        }
        t.no_damage = self.static_obj("NoDamageConsoleCommand", "main").unwrap_or(0);
        t.no_cost = self.static_obj("NoCostConsoleCommand", "main").unwrap_or(0);
        t.live = self.ptr_at(player, "Player", "liveMixin").unwrap_or(0);
        if t.live != 0 {
            t.health = t.live + self.f("LiveMixin", "health").unwrap_or(0);
            if let (Some(data), Some(o)) = (self.ptr_at(t.live, "LiveMixin", "data"), self.f("LiveMixinData", "maxHealth")) {
                t.max_health = data + o;
            }
        }
        t.oxygen_mgr = self.ptr_at(player, "Player", "oxygenMgr").unwrap_or(0);
        if let Some(pc) = self.ptr_at(player, "Player", "<playerController>k__BackingField") {
            t.controller = pc;
            t.swim_motor = self.ptr_at(pc, "PlayerController", "underWaterController").unwrap_or(0);
            t.walk_motor = self.ptr_at(pc, "PlayerController", "groundController").unwrap_or(0);
        }
        // Survival has no static and the player doesn't point to it: find the one whose player is ours.
        if let Some(c) = self.m.class("", "Survival") {
            if let Some(po) = self.f("Survival", "player") {
                t.survival = self.m.instances(c, 64).into_iter().find(|&s| self.m.p.ptr(s + po) == Some(player)).unwrap_or(0);
            }
        }
        log(&format!("sn1: player 0x{player:X} cheats 0x{:X} survival 0x{:X}", t.cheats, t.survival));
        self.t = t;
    }

    fn switch(&mut self, addr: u64, on: bool) {
        if addr == 0 {
            return;
        }
        if on {
            if !self.switched.contains_key(&addr) {
                let Some(v) = self.m.p.read::<u8>(addr) else { return };
                self.switched.insert(addr, v);
            }
            self.m.p.write(addr, 1u8);
        } else if let Some(v) = self.switched.remove(&addr) {
            self.m.p.write(addr, v);
        }
    }

    pub fn tick(&mut self, values: Option<&HashMap<String, f64>>) -> Result<bool, String> {
        let Some(c) = self.m.class("", "Player") else { return Err("Game closed".into()) };
        let Some(main) = self.m.static_addr(c, "main") else { return Err("Game closed".into()) };
        let Some(player) = self.m.p.read::<u64>(main) else { return Err("Game closed".into()) };
        if player == 0 {
            return Ok(false); // main menu or loading
        }
        if player != self.t.player {
            self.switched.clear();
            self.speed_orig.clear();
            self.speed_last = (1.0, 1.0);
            self.resolve(player);
        }
        let none = HashMap::new();
        let v = values.unwrap_or(&none);
        let on = |k: &str| v.get(k).is_some_and(|x| *x != 0.0);
        let t = self.t;

        // Game cheat flags: on while the mod is on, otherwise whatever the game mode itself has
        // (so a flag left on by an earlier Hotswap run is cleared too).
        if let Some(cur) = self.m.p.read::<u32>(t.cheats).filter(|_| t.cheats != 0) {
            let mode = self.m.p.read::<u32>(t.mode).filter(|_| t.mode != 0).unwrap_or(0);
            let mut want = cur;
            for (k, bit) in FLAGS {
                if on(k) || mode & bit != 0 { want |= bit } else { want &= !bit }
            }
            if want != cur {
                self.m.p.write(t.cheats, want);
            }
        }
        // Console cheats.
        let god = on("infinite-health");
        if t.no_damage != 0 {
            let o = self.f("NoDamageConsoleCommand", "noDamageCheat").unwrap_or(0);
            self.switch(t.no_damage + o, god);
        }
        if god && t.health != 0 {
            let max = self.m.p.read::<f32>(t.max_health).filter(|m| *m > 0.0).unwrap_or(100.0);
            if self.m.p.read::<f32>(t.health).is_some_and(|h| h < max) {
                self.m.p.write(t.health, max);
            }
        }
        if t.no_cost != 0 {
            for (k, field) in [("instant-building", "<fastBuildCheat>k__BackingField"), ("instant-scanning", "<fastScanCheat>k__BackingField"),
                ("fast-growing", "<fastGrowCheat>k__BackingField"), ("fast-growing", "<fastHatchCheat>k__BackingField")] {
                if let Some(o) = self.f("NoCostConsoleCommand", field) {
                    self.switch(t.no_cost + o, on(k));
                }
            }
        }
        // Food and water.
        if t.survival != 0 {
            for (k, field) in [("no-hunger", "food"), ("no-thirst", "water")] {
                if on(k) {
                    if let Some(o) = self.f("Survival", field) {
                        if self.m.p.read::<f32>(t.survival + o).is_some_and(|x| x < 99.0) {
                            self.m.p.write(t.survival + o, 100.0f32);
                        }
                    }
                }
            }
        }
        // Oxygen: the game's NoOxygen cheat only stops it going down, so also fill every oxygen
        // source (lungs and tanks) to its capacity.
        if on("infinite-oxygen") && t.oxygen_mgr != 0 {
            self.fill_oxygen(t.oxygen_mgr);
        }
        self.speeds(t, (v.get("swim-speed").copied().unwrap_or(100.0) / 100.0) as f32, (v.get("walk-speed").copied().unwrap_or(100.0) / 100.0) as f32);
        Ok(true)
    }

    fn fill_oxygen(&mut self, mgr: u64) {
        let (Some(list), Some(cap_o), Some(avail_o)) = (self.ptr_at(mgr, "OxygenManager", "sources"), self.f("Oxygen", "oxygenCapacity"), self.f("Oxygen", "oxygenAvailable")) else { return };
        // List<T>: _items (an array: header, length, then elements at +0x20) and _size.
        let (Some(items), Some(n)) = (self.m.p.ptr(list + 0x10), self.m.p.read::<i32>(list + 0x18)) else { return };
        let oxygen_vt = self.m.class("", "Oxygen").and_then(|c| self.m.vtable(c));
        for i in 0..n.clamp(0, 16) as u64 {
            let Some(src) = self.m.p.ptr(items + 0x20 + i * 8) else { continue };
            if oxygen_vt.is_some() && self.m.p.read::<u64>(src) != oxygen_vt {
                continue;
            }
            if let Some(cap) = self.m.p.read::<f32>(src + cap_o).filter(|c| *c > 0.0 && *c < 10_000.0) {
                if self.m.p.read::<f32>(src + avail_o).is_some_and(|a| a < cap) {
                    self.m.p.write(src + avail_o, cap);
                }
            }
        }
    }

    /// Scales the player's speed limits: the controller's defaults (copied into the motors when the
    /// player switches between swimming, the Seaglide and walking) and the values in use now.
    fn speeds(&mut self, t: Targets, swim: f32, walk: f32) {
        if t.controller == 0 || (swim, walk) == self.speed_last {
            return;
        }
        // An earlier Hotswap run may have scaled these already in this same game session; its
        // record (kept per game process) holds the real values and what was applied.
        if self.speed_orig.is_empty() {
            if let Some((orig, last)) = speed_file_load(self.m.p.pid) {
                self.speed_orig = orig;
                self.speed_last = last;
            }
        }
        let (old_swim, old_walk) = self.speed_last;
        for (pc_field, _, _) in SPEEDS {
            if !self.speed_orig.contains_key(pc_field) {
                let Some(o) = self.f("PlayerController", pc_field) else { continue };
                // Already scaled by an earlier session is impossible to tell apart; read it as is.
                let Some(v) = self.m.p.read::<f32>(t.controller + o) else { continue };
                self.speed_orig.insert(pc_field, v);
            }
        }
        let factor = |kind: &str, s: f32, w: f32| if kind == "walk" { w } else { s };
        for (pc_field, _, kind) in SPEEDS {
            let Some(o) = self.f("PlayerController", pc_field) else { continue };
            let Some(&base) = self.speed_orig.get(pc_field) else { continue };
            self.m.p.write(t.controller + o, base * factor(kind, swim, walk));
        }
        // The motors hold a copy for the current mode: rescale whatever is in there now.
        for (motor, old, new) in [(t.swim_motor, old_swim, swim), (t.walk_motor, old_walk, walk)] {
            if motor == 0 || old <= 0.0 {
                continue;
            }
            // In water the top speed is acceleration against drag, so acceleration scales too.
            for field in ["forwardMaxSpeed", "backwardMaxSpeed", "strafeMaxSpeed", "verticalMaxSpeed", "waterAcceleration", "groundAcceleration"] {
                if let Some(o) = self.f("PlayerMotor", field) {
                    if let Some(cur) = self.m.p.read::<f32>(motor + o) {
                        self.m.p.write(motor + o, cur / old * new);
                    }
                }
            }
        }
        self.speed_last = (swim, walk);
        speed_file_save(self.m.p.pid, &self.speed_orig, self.speed_last);
        // An earlier version used the motors' debug multiplier; put it back to normal.
        if let Some(o) = self.f("PlayerMotor", "debugSpeedMult") {
            for motor in [t.swim_motor, t.walk_motor] {
                if motor != 0 && self.m.p.read::<f32>(motor + o).is_some_and(|x| x != 1.0) {
                    self.m.p.write(motor + o, 1.0f32);
                }
            }
        }
    }
}

fn speed_file() -> Option<std::path::PathBuf> {
    Some(std::path::Path::new(&std::env::var("APPDATA").ok()?).join("app.hotswap.desktop").join("subnautica_speeds.json"))
}

/// The real speed values and the factors applied, if recorded for this game process.
fn speed_file_load(pid: u32) -> Option<(HashMap<&'static str, f32>, (f32, f32))> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(speed_file()?).ok()?).ok()?;
    if v["pid"].as_u64()? != pid as u64 {
        return None;
    }
    let orig = SPEEDS.iter().filter_map(|(k, _, _)| Some((*k, v["orig"][*k].as_f64()? as f32))).collect();
    Some((orig, (v["swim"].as_f64()? as f32, v["walk"].as_f64()? as f32)))
}

fn speed_file_save(pid: u32, orig: &HashMap<&'static str, f32>, last: (f32, f32)) {
    if let Some(p) = speed_file() {
        let _ = std::fs::write(p, serde_json::json!({ "pid": pid, "orig": orig, "swim": last.0, "walk": last.1 }).to_string());
    }
}
