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
    /// The highest a game value has been this session (refills go back up to it).
    best: HashMap<u64, f32>,
    /// Names read from game objects, by address.
    names: HashMap<u64, String>,
}

/// Where a survival kit Value<float> keeps its number (after two delegate fields).
const VALUE_CURRENT: u64 = 0x20;

impl Session {
    pub fn new(game: &'static str, m: Mono) -> Session {
        Session { game, m, off: HashMap::new(), bools: HashMap::new(), floats: HashMap::new(), ints: HashMap::new(), found: HashMap::new(), best: HashMap::new(), names: HashMap::new() }
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
    /// Keeps a game number at `factor` times its own value (put back at 1).
    fn scale(&mut self, addr: u64, factor: f32) {
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
        self.oni_duplicants(v);
        // Instant research: the current research's progress is filled, so it completes the next time
        // a duplicant adds a point.
        if on("instant-research") {
            if let Some(r) = self.static_obj("", "Research", "Instance") {
                if let Some(inv) = self.ptr(r, "", "Research", "activeResearch").and_then(|t| self.ptr(t, "", "TechInstance", "progressInventory")) {
                    if let Some(dict) = self.ptr(inv, "", "ResearchPointInventory", "PointsByTypeID") {
                        self.fill_dict_floats(dict, 1_000_000.0);
                    }
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

    /// Duplicant needs: each is an AmountInstance (value) whose Amount has an Id ("Stress", ...).
    /// Only amounts on duplicants are touched: their game objects are the ones that have Stress.
    fn oni_duplicants(&mut self, v: &HashMap<String, f64>) {
        let on = |k: &str| v.get(k).is_some_and(|x| *x != 0.0);
        const WANT: [(&str, &str, bool); 6] = [
            ("no-stress", "Stress", false), ("no-bladder", "Bladder", false),
            ("no-hunger", "Calories", true), ("infinite-breath", "Breath", true), ("no-fatigue", "Stamina", true), ("full-health", "HitPoints", true),
        ];
        if !WANT.iter().any(|(k, _, _)| on(k)) {
            return;
        }
        const AI: &str = "Klei.AI";
        let (Some(vo), Some(mo), Some(go), Some(io)) = (
            self.f(AI, "AmountInstance", "value"),
            self.f(AI, "AmountInstance", "modifier"),
            self.f(AI, "AmountInstance", "<gameObject>k__BackingField"),
            self.f(AI, "Amount", "Id"),
        ) else { return };
        let amounts = self.objects(AI, "AmountInstance", Duration::from_secs(15), 20_000);
        // Amount names, read once per Amount.
        let mut list = Vec::with_capacity(amounts.len());
        for a in amounts {
            let (Some(amount), Some(obj)) = (self.m.p.ptr(a + mo), self.m.p.ptr(a + go)) else { continue };
            let id = match self.names.get(&amount) {
                Some(n) => n.clone(),
                None => {
                    let n = self.m.p.ptr(amount + io).map(|s| self.m.string(s)).unwrap_or_default();
                    self.names.insert(amount, n.clone());
                    n
                }
            };
            list.push((a, obj, id));
        }
        let dupes: std::collections::HashSet<u64> = list.iter().filter(|(_, _, id)| id == "Stress").map(|(_, o, _)| *o).collect();
        for (a, obj, id) in list {
            if !dupes.contains(&obj) {
                continue;
            }
            let Some(&(k, _, up)) = WANT.iter().find(|(_, name, _)| *name == id) else { continue };
            let addr = a + vo;
            let Some(cur) = self.m.p.read::<f32>(addr).filter(|x| x.is_finite()) else { continue };
            if up {
                // Kept at the highest it's been (needs bars start full).
                let best = self.best.entry(addr).or_insert(cur);
                *best = best.max(cur);
                let b = *best;
                if on(k) && cur < b {
                    self.m.p.write(addr, b);
                }
            } else if on(k) && cur > 0.0 {
                self.m.p.write(addr, 0.0f32);
            }
        }
    }

    /// Sets every value of a Dictionary<string, float> to at least `min` (Mono's layout: entries
    /// array at +0x18; each entry is hash, next, key, value: 24 bytes from +0x20 in the array).
    fn fill_dict_floats(&mut self, dict: u64, min: f32) {
        let Some(entries) = self.m.p.ptr(dict + 0x18).filter(|&e| e != 0) else { return };
        let Some(len) = self.m.p.read::<u64>(entries + 0x18).filter(|&l| l > 0 && l < 64) else { return };
        for i in 0..len {
            let e = entries + 0x20 + i * 24;
            // Unused slots have no key.
            if self.m.p.ptr(e + 8).is_some_and(|k| k != 0) {
                if self.m.p.read::<f32>(e + 16).is_some_and(|x| x.is_finite() && x < min) {
                    self.m.p.write(e + 16, min);
                }
            }
        }
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
        // Walking and jumping, as a percentage of normal.
        for (key, field) in [("walk-speed", "movementNormalSpeed"), ("jump-height", "jumpSpeed")] {
            let factor = (v.get(key).copied().unwrap_or(100.0) / 100.0) as f32;
            for w in self.objects("CMF", "AdvancedWalkerController", Duration::from_secs(5), 4) {
                let Some(o) = self.f("CMF", "AdvancedWalkerController", field) else { break };
                self.scale(w + o, factor);
            }
        }
        // Time: UniStorm's time flow switch (Enabled 0, Disabled 1) and the time of day (0..1).
        if let Some(sky) = self.static_obj("UniStorm", "UniStormSystem", "Instance") {
            let (day, freeze) = (on("always-day"), on("freeze-time"));
            if let Some(o) = self.f("UniStorm", "UniStormSystem", "TimeFlow") {
                let addr = sky + o;
                if day || freeze {
                    if !self.ints.contains_key(&addr) {
                        if let Some(x) = self.m.p.read::<i32>(addr) {
                            self.ints.insert(addr, x);
                        }
                    }
                    if self.m.p.read::<i32>(addr) != Some(1) {
                        self.m.p.write(addr, 1i32);
                    }
                } else if let Some(x) = self.ints.remove(&addr) {
                    self.m.p.write(addr, x);
                }
            }
            if day {
                if let Some(o) = self.f("UniStorm", "UniStormSystem", "m_TimeFloat") {
                    if self.m.p.read::<f32>(sky + o).is_some_and(|t| (t - 0.5).abs() > 0.01) {
                        self.m.p.write(sky + o, 0.5f32);
                    }
                }
            }
        }
        // Demolition derby: your car takes no damage.
        for d in self.objects("", "Event_DerbyDemolition_Vehicle", Duration::from_secs(10), 32) {
            if let Some(o) = self.f("", "Event_DerbyDemolition_Vehicle", "canTakeDamage") {
                self.hold_u8(d + o, on("no-derby-damage"), 0);
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
        // Oxygen: no suffocation damage, and lungs and bottles kept at the fullest they've been.
        let oxy_on = on("infinite-oxygen");
        for u in self.objects("", "PlayerOxygenUsage", Duration::from_secs(5), 4) {
            if let Some(o) = self.f("", "PlayerOxygenUsage", "damageWithoutOxygen") {
                self.hold_f32(u + o, oxy_on, 0.0);
            }
            for field in ["playerOxygenLevel", "itemOxygenLevel"] {
                let Some(o) = self.f("", "PlayerOxygenUsage", field) else { continue };
                let Some(cur) = self.m.p.read::<f32>(u + o).filter(|v| v.is_finite()) else { continue };
                let best = self.best.entry(u + o).or_insert(cur);
                *best = best.max(cur);
                if oxy_on && cur < *best {
                    let b = *best;
                    self.m.p.write(u + o, b);
                }
            }
        }
        // Suit: its own "don't use energy" switch and a full battery; radiation kept at zero.
        for sp in self.objects("", "PlayerAndSuitParams", Duration::from_secs(5), 4) {
            if let Some(o) = self.f("", "PlayerAndSuitParams", "notUseEnergySuit") {
                self.hold_u8(sp + o, on("infinite-suit-energy"), 1);
            }
            if on("infinite-suit-energy") {
                if let (Some(e), Some(mo)) = (self.ptr(sp, "", "PlayerAndSuitParams", "energy"), self.f("", "PlayerAndSuitParams", "maxEnergy")) {
                    if let Some(max) = self.m.p.read::<f32>(sp + mo).filter(|m| *m > 0.0 && *m < 1e6) {
                        self.fill_f32(e + VALUE_CURRENT, max);
                    }
                }
            }
            if on("no-radiation") {
                if let Some(r) = self.ptr(sp, "", "PlayerAndSuitParams", "radiation") {
                    if self.m.p.read::<f32>(r + VALUE_CURRENT).is_some_and(|x| x > 0.0) {
                        self.m.p.write(r + VALUE_CURRENT, 0.0f32);
                    }
                }
            }
        }
        // Rover batteries stay charged.
        if on("infinite-rover-battery") {
            const CAR: &str = "UnityStandardAssets.Vehicles.Car";
            let (lo, mo) = (self.f(CAR, "BatteryController", "currentBatteryLevel"), self.f(CAR, "BatteryController", "maxBatteryWorkingTime"));
            for b in self.objects(CAR, "BatteryController", Duration::from_secs(10), 32) {
                if let (Some(lo), Some(mo)) = (lo, mo) {
                    if let Some(max) = self.m.p.read::<f32>(b + mo).filter(|m| *m > 0.0 && *m < 1e7) {
                        self.fill_f32(b + lo, max);
                    }
                }
            }
        }
        // Movement speed, as a percentage of the player's own.
        let speed = (v.get("movement-speed").copied().unwrap_or(100.0) / 100.0) as f32;
        for a in self.objects("", "PlayerAnimationController", Duration::from_secs(5), 4) {
            let Some(o) = self.f("", "PlayerAnimationController", "playerSpeedModifier") else { break };
            let addr = a + o;
            if speed != 1.0 {
                if !self.floats.contains_key(&addr) {
                    if let Some(base) = self.m.p.read::<f32>(addr) {
                        self.floats.insert(addr, base);
                    }
                }
                if let Some(&base) = self.floats.get(&addr) {
                    self.hold_f32(addr, true, base * speed);
                }
            } else {
                self.hold_f32(addr, false, 0.0);
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
