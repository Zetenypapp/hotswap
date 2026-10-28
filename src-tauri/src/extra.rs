//! Three smaller Unity games on Mono (see mono.rs), each a handful of mods:
//! - Oxygen Not Included (457140): the game's own debug and sandbox switches.
//! - Mon Bazou (1520370): needs, cash and fuel, on the game's manager objects.
//! - Occupy Mars (758690): its immortality and free-build switches, and the player's hunger, thirst
//!   and stamina drain.
//! Objects without a static handle are found in the game's memory by their class (cached, and
//! looked up again when they go away, e.g. after loading another save).

use crate::{mono::Mono, trainer::log};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub const ONI: &str = "457140";
pub const MON_BAZOU: &str = "1520370";
pub const OCCUPY_MARS: &str = "758690";

pub struct Session {
    game: &'static str,
    m: Mono,
    off: HashMap<(String, String), u64>,
    /// Game values we changed: address -> the value before.
    bools: HashMap<u64, u8>,
    floats: HashMap<u64, f32>,
    ints: HashMap<u64, i32>,
    /// Objects found by class: class -> (objects, when last searched).
    found: HashMap<String, (Vec<u64>, Instant)>,
}

impl Session {
    pub fn new(game: &'static str, m: Mono) -> Session {
        Session { game, m, off: HashMap::new(), bools: HashMap::new(), floats: HashMap::new(), ints: HashMap::new(), found: HashMap::new() }
    }

    fn f(&mut self, ns: &str, class: &str, field: &str) -> Option<u64> {
        let key = (format!("{ns}.{class}"), field.to_string());
        if let Some(&o) = self.off.get(&key) {
            return Some(o);
        }
        let c = self.m.class(ns, class)?;
        let o = self.m.field(c, field)? as u64;
        self.off.insert(key, o);
        Some(o)
    }
    fn static_addr(&mut self, ns: &str, class: &str, field: &str) -> Option<u64> {
        let c = self.m.class(ns, class)?;
        self.m.static_addr(c, field)
    }
    fn static_obj(&mut self, ns: &str, class: &str, field: &str) -> Option<u64> {
        let a = self.static_addr(ns, class, field)?;
        self.m.p.ptr(a).filter(|&o| o != 0)
    }
    fn ptr(&mut self, obj: u64, ns: &str, class: &str, field: &str) -> Option<u64> {
        let o = self.f(ns, class, field)?;
        self.m.p.ptr(obj + o).filter(|&p| p != 0)
    }

    /// Live objects of a class. Searching memory is slow, so the result is kept while its objects
    /// are still alive, and searched again at most every `every`.
    fn objects(&mut self, ns: &str, class: &str, every: Duration, limit: usize) -> Vec<u64> {
        let key = format!("{ns}.{class}");
        let Some(c) = self.m.class(ns, class) else { return vec![] };
        let vt = self.m.vtable(c);
        if let Some((objs, at)) = self.found.get(&key) {
            let alive: Vec<u64> = objs.iter().copied().filter(|&o| vt.is_some() && self.m.p.read::<u64>(o) == vt).collect();
            if (!alive.is_empty() && alive.len() == objs.len()) || at.elapsed() < every {
                return alive;
            }
        }
        let objs = self.m.instances(c, limit);
        log(&format!("{}: found {} {key}", self.game, objs.len()));
        self.found.insert(key, (objs.clone(), Instant::now()));
        objs
    }

    /// Keeps a game value at `want` while `on`, and puts back the old value when turned off.
    fn hold_u8(&mut self, addr: u64, on: bool, want: u8) {
        if on {
            if !self.bools.contains_key(&addr) {
                let Some(v) = self.m.p.read::<u8>(addr) else { return };
                self.bools.insert(addr, v);
            }
            if self.m.p.read::<u8>(addr) != Some(want) {
                self.m.p.write(addr, want);
            }
        } else if let Some(v) = self.bools.remove(&addr) {
            self.m.p.write(addr, v);
        }
    }
    fn hold_f32(&mut self, addr: u64, on: bool, want: f32) {
        if on {
            if !self.floats.contains_key(&addr) {
                let Some(v) = self.m.p.read::<f32>(addr) else { return };
                self.floats.insert(addr, v);
            }
            if self.m.p.read::<f32>(addr).is_some_and(|v| v != want) {
                self.m.p.write(addr, want);
            }
        } else if let Some(v) = self.floats.remove(&addr) {
            self.m.p.write(addr, v);
        }
    }
    /// Raises a game number to at least `min` (no putting back: a refill).
    fn fill_f32(&mut self, addr: u64, min: f32) {
        if self.m.p.read::<f32>(addr).is_some_and(|v| v < min) {
            self.m.p.write(addr, min);
        }
    }

    pub fn tick(&mut self, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Result<bool, String> {
        let none = HashMap::new();
        let v = values.unwrap_or(&none);
        match self.game {
            ONI => self.oni(v),
            MON_BAZOU => self.mon_bazou(v, actions),
            _ => self.mars(v),
        }
    }

    fn oni(&mut self, v: &HashMap<String, f64>) -> Result<bool, String> {
        let on = |k: &str| v.get(k).is_some_and(|x| *x != 0.0);
        if self.m.class("", "DebugHandler").is_none() {
            return Err("Game closed".into());
        }
        // In a colony once the save is loaded.
        let Some(save) = self.static_obj("", "SaveGame", "Instance") else { return Ok(false) };
        if let Some(a) = self.static_addr("", "DebugHandler", "InstantBuildMode") {
            self.hold_u8(a, on("instant-build"), 1);
        }
        if let Some(o) = self.f("", "SaveGame", "sandboxEnabled") {
            self.hold_u8(save + o, on("sandbox-tools"), 1);
        }
        if on("printing-pod-always-ready") {
            if let (Some(imm), Some(o)) = (self.static_obj("", "Immigration", "Instance"), self.f("", "Immigration", "timeBeforeSpawn")) {
                if self.m.p.read::<f32>(imm + o).is_some_and(|t| t > 0.0) {
                    self.m.p.write(imm + o, 0.0f32);
                }
            }
        }
        // The fastest speed button, as a percentage of its normal speed.
        if let (Some(scr), Some(o)) = (self.static_obj("", "SpeedControlScreen", "<Instance>k__BackingField"), self.f("", "SpeedControlScreen", "ultraSpeed")) {
            let factor = (v.get("max-game-speed").copied().unwrap_or(100.0) / 100.0) as f32;
            let addr = scr + o;
            if factor != 1.0 {
                if !self.floats.contains_key(&addr) {
                    if let Some(base) = self.m.p.read::<f32>(addr) {
                        self.floats.insert(addr, base);
                    }
                }
                if let Some(&base) = self.floats.get(&addr) {
                    self.hold_f32(addr, true, base * factor);
                }
            } else {
                self.hold_f32(addr, false, 0.0);
            }
        }
        Ok(true)
    }

    fn mon_bazou(&mut self, v: &HashMap<String, f64>, actions: &[String]) -> Result<bool, String> {
        let on = |k: &str| v.get(k).is_some_and(|x| *x != 0.0);
        if self.m.class("", "BasicNeeds_Manager").is_none() {
            return Err("Game closed".into());
        }
        let needs = self.objects("", "BasicNeeds_Manager", Duration::from_secs(5), 4);
        if needs.is_empty() {
            return Ok(false); // main menu or loading
        }
        // Needs bars run 0..100, full is good.
        for n in needs {
            for (k, field) in [("no-hunger", "Poutine"), ("no-thirst", "MappleSyrup"), ("no-sleep", "Energy"), ("no-anxiety", "Anxiety")] {
                if on(k) {
                    if let Some(o) = self.f("", "BasicNeeds_Manager", field) {
                        self.fill_f32(n + o, 100.0);
                    }
                }
            }
        }
        // Cash: topped up while on, your real balance back when off.
        for c in self.objects("", "Cash_Manager", Duration::from_secs(5), 4) {
            let Some(o) = self.f("", "Cash_Manager", "cash") else { break };
            let addr = c + o;
            if on("unlimited-money") {
                if !self.ints.contains_key(&addr) {
                    if let Some(x) = self.m.p.read::<i32>(addr) {
                        self.ints.insert(addr, x);
                    }
                }
                if self.m.p.read::<i32>(addr).is_some_and(|x| x < 999_999) {
                    self.m.p.write(addr, 999_999i32);
                }
            } else if let Some(x) = self.ints.remove(&addr) {
                self.m.p.write(addr, x);
            }
            if actions.iter().any(|a| a == "add-10-000") {
                let x = self.m.p.read::<i32>(addr).unwrap_or(0);
                self.m.p.write(addr, x.saturating_add(10_000));
                if let Some(base) = self.ints.get_mut(&addr) {
                    *base = base.saturating_add(10_000);
                }
            }
        }
        if on("infinite-fuel") {
            let tanks = self.objects("", "Part_FuelTank", Duration::from_secs(10), 64);
            if let (Some(lo), Some(co)) = (self.f("", "Part_FuelTank", "Level"), self.f("", "Part_FuelTank", "Capacity")) {
                for t in tanks {
                    if let Some(cap) = self.m.p.read::<f32>(t + co).filter(|c| *c > 0.0 && *c < 10_000.0) {
                        self.fill_f32(t + lo, cap);
                    }
                }
            }
        }
        Ok(true)
    }

    fn mars(&mut self, v: &HashMap<String, f64>) -> Result<bool, String> {
        let on = |k: &str| v.get(k).is_some_and(|x| *x != 0.0);
        const US: &str = "UltimateSurvival";
        if self.m.class(US, "PlayerVitals").is_none() {
            return Err("Game closed".into());
        }
        let vitals = self.objects(US, "PlayerVitals", Duration::from_secs(5), 4);
        if vitals.is_empty() {
            return Ok(false); // main menu or loading
        }
        for gc in self.objects("UltimateSurvivalMod", "GameControllerSecond", Duration::from_secs(5), 4) {
            if let Some(o) = self.f("UltimateSurvivalMod", "GameControllerSecond", "cheatImmortalityActive") {
                self.hold_u8(gc + o, on("immortality"), 1);
            }
        }
        // Free building: the building system's own switch (no materials needed).
        const BUILD: &str = "UltimateSurvival.Building";
        for class in ["PlayerBuilder", "BuildingHelpers"] {
            for b in self.objects(BUILD, class, Duration::from_secs(5), 8) {
                if let Some(o) = self.f(BUILD, class, "freeBuild") {
                    self.hold_u8(b + o, on("free-building"), 1);
                }
            }
        }
        for p in vitals {
            // Hunger and thirst drain (and the damage when empty) stop while on.
            for (k, field) in [("no-hunger", "m_HungerDepletion"), ("no-thirst", "m_ThirstDepletion")] {
                if let Some(d) = self.ptr(p, US, "PlayerVitals", field) {
                    for f in ["m_DepletionRate", "m_Damage"] {
                        if let Some(o) = self.f(US, "StatDepleter", f) {
                            self.hold_f32(d + o, on(k), 0.0);
                        }
                    }
                }
            }
            for f in ["m_StaminaDepletionRate", "m_JumpStaminaTake"] {
                if let Some(o) = self.f(US, "PlayerVitals", f) {
                    self.hold_f32(p + o, on("infinite-stamina"), 0.0);
                }
            }
        }
        Ok(true)
    }
}
