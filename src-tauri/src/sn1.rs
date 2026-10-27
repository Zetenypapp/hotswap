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
    no_damage: u64,
    no_cost: u64,
    live: u64,
    health: u64,
    max_health: u64,
    survival: u64,
    swim_motor: u64,
    walk_motor: u64,
}

pub struct Session {
    m: Mono,
    t: Targets,
    off: HashMap<&'static str, u64>,
    /// Cheat bits we switched on (only those are switched back off).
    ours: u32,
    /// Console cheat switches we turned on: address -> the value before.
    switched: HashMap<u64, u8>,
}

impl Session {
    pub fn new(m: Mono) -> Session {
        Session { m, t: Targets::default(), off: HashMap::new(), ours: 0, switched: HashMap::new() }
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
        if let Some(pc) = self.ptr_at(player, "Player", "<playerController>k__BackingField") {
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
            self.ours = 0;
            self.switched.clear();
            self.resolve(player);
        }
        let none = HashMap::new();
        let v = values.unwrap_or(&none);
        let on = |k: &str| v.get(k).is_some_and(|x| *x != 0.0);
        let t = self.t;

        // Game cheat flags.
        if let Some(cur) = self.m.p.read::<u32>(t.cheats).filter(|_| t.cheats != 0) {
            let mut want = cur;
            for (k, bit) in FLAGS {
                if on(k) && cur & bit == 0 {
                    want |= bit;
                    self.ours |= bit;
                } else if !on(k) && self.ours & bit != 0 {
                    want &= !bit;
                    self.ours &= !bit;
                }
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
        // Speeds: the motors' own speed multiplier.
        for (k, motor) in [("swim-speed", t.swim_motor), ("walk-speed", t.walk_motor)] {
            if motor == 0 {
                continue;
            }
            let want = (v.get(k).copied().unwrap_or(100.0) / 100.0) as f32;
            if let Some(o) = self.f("PlayerMotor", "debugSpeedMult") {
                if self.m.p.read::<f32>(motor + o).is_some_and(|x| (x - want).abs() > 1e-3) {
                    self.m.p.write(motor + o, want);
                }
            }
        }
        Ok(true)
    }
}
