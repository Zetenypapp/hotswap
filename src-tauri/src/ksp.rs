//! Kerbal Space Program (Steam app 220200), a Unity game on Mono (see mono.rs). Uses the game's own
//! cheat switches (its Alt+F12 menu) where it has them, and refills the active vessel's resources.

use crate::{mono::Mono, trainer::log};
use std::collections::HashMap;

pub const KSP: &str = "220200";

/// Resources refilled by each mod. "Fuel" also takes any other resource with Fuel or Propellant
/// in its name (mod parts), except monopropellant.
const FUELS: [&str; 5] = ["LiquidFuel", "Oxidizer", "SolidFuel", "XenonGas", "EVA Propellant"];
const FLIGHT: [&str; 2] = ["ElectricCharge", "Ablator"];

/// Career numbers kept topped up while their mod is on.
const FUNDS: f64 = 99_999_999.0;
const SCIENCE: f32 = 999_999.0;

pub struct Session {
    m: Mono,
    off: HashMap<(&'static str, &'static str), u64>,
    /// Game switches we turned on: address -> the value before.
    switched: HashMap<u64, u8>,
    /// Funds, science and reputation before their mod was turned on (put back when it's off),
    /// with the object they belong to (a new one after loading another save).
    funds_orig: Option<(u64, f64)>,
    science_orig: Option<(u64, f32)>,
    rep_orig: Option<(u64, f32)>,
    last_vessel: u64,
    /// Parts made tough by Invincible Craft: part -> its crash tolerance, g tolerance and max pressure.
    tough: HashMap<u64, (f32, f64, f64)>,
}

impl Session {
    pub fn new(m: Mono) -> Session {
        Session { m, off: HashMap::new(), switched: HashMap::new(), funds_orig: None, science_orig: None, rep_orig: None, last_vessel: 0, tough: HashMap::new() }
    }

    fn f(&mut self, class: &'static str, field: &'static str) -> Option<u64> {
        if let Some(&o) = self.off.get(&(class, field)) {
            return Some(o);
        }
        let c = self.m.class("", class)?;
        let o = self.m.field(c, field)? as u64;
        self.off.insert((class, field), o);
        Some(o)
    }
    fn static_addr(&mut self, class: &str, field: &str) -> Option<u64> {
        let c = self.m.class("", class)?;
        self.m.static_addr(c, field)
    }
    fn static_obj(&mut self, class: &str, field: &str) -> Option<u64> {
        let a = self.static_addr(class, field)?;
        self.m.p.ptr(a).filter(|&o| o != 0)
    }

    /// Keeps a game bool on while `on`, and puts back its old value when turned off.
    fn switch_at(&mut self, addr: u64, on: bool) {
        if on {
            if !self.switched.contains_key(&addr) {
                let Some(v) = self.m.p.read::<u8>(addr) else { return };
                self.switched.insert(addr, v);
            }
            if self.m.p.read::<u8>(addr) != Some(1) {
                self.m.p.write(addr, 1u8);
            }
        } else if let Some(v) = self.switched.remove(&addr) {
            self.m.p.write(addr, v);
        }
    }
    fn cheat(&mut self, field: &str, on: bool) {
        if let Some(a) = self.static_addr("CheatOptions", field) {
            self.switch_at(a, on);
        }
    }

    /// Every part of the vessel you're flying.
    fn vessel_parts(&mut self) -> Vec<u64> {
        let Some(fg) = self.static_obj("FlightGlobals", "_fetch") else { return vec![] };
        let Some(vessel) = self.f("FlightGlobals", "activeVessel").and_then(|o| self.m.p.ptr(fg + o)).filter(|&v| v != 0) else { return vec![] };
        let Some(po) = self.f("Vessel", "parts") else { return vec![] };
        let Some(parts) = self.m.p.ptr(vessel + po).filter(|&l| l != 0) else { return vec![] };
        let (Some(items), Some(n)) = (self.m.p.ptr(parts + 0x10), self.m.p.read::<i32>(parts + 0x18)) else { return vec![] };
        if vessel != self.last_vessel {
            self.last_vessel = vessel;
            log(&format!("ksp: active vessel 0x{vessel:X}, {n} parts"));
        }
        (0..n.clamp(0, 2000) as u64).filter_map(|i| self.m.p.ptr(items + 0x20 + i * 8).filter(|&p| p != 0)).collect()
    }

    /// Every resource on the parts: (address of the PartResource, its name).
    fn vessel_resources(&mut self, parts: &[u64]) -> Vec<(u64, String)> {
        let mut out = Vec::new();
        let (Some(ro), Some(no)) = (self.f("Part", "_resources"), self.f("PartResource", "resourceName")) else { return out };
        // DictionaryValueList<int, PartResource> keeps its values in a List as well.
        let dict_o = self.f("PartResourceList", "dict").unwrap_or(0x10);
        let list_o = self.f("DictionaryValueList`2", "list").unwrap_or(0x18);
        for &part in parts {
            let Some(list) = self.m.p.ptr(part + ro).and_then(|r| self.m.p.ptr(r + dict_o)).and_then(|d| self.m.p.ptr(d + list_o)).filter(|&l| l != 0) else { continue };
            let (Some(ritems), Some(rn)) = (self.m.p.ptr(list + 0x10), self.m.p.read::<i32>(list + 0x18)) else { continue };
            for j in 0..rn.clamp(0, 64) as u64 {
                let Some(res) = self.m.p.ptr(ritems + 0x20 + j * 8).filter(|&r| r != 0) else { continue };
                let name = self.m.p.ptr(res + no).map(|s| self.m.string(s)).unwrap_or_default();
                out.push((res, name));
            }
        }
        out
    }

    /// Invincible Craft: parts shrug off impacts, g-forces and pressure. The game's own No Crash
    /// Damage switch doesn't stop every impact, so each part's limits go up too (put back when off).
    fn toughen(&mut self, parts: &[u64], on: bool) {
        let (Some(c), Some(g), Some(pr)) = (self.f("Part", "crashTolerance"), self.f("Part", "gTolerance"), self.f("Part", "maxPressure")) else { return };
        if on {
            for &part in parts {
                if !self.tough.contains_key(&part) {
                    let (Some(oc), Some(og), Some(op)) = (self.m.p.read::<f32>(part + c), self.m.p.read::<f64>(part + g), self.m.p.read::<f64>(part + pr)) else { continue };
                    self.tough.insert(part, (oc, og, op));
                }
                if self.m.p.read::<f32>(part + c).is_some_and(|v| v < 1e5) {
                    self.m.p.write(part + c, 1e6f32);
                    self.m.p.write(part + g, 1e9f64);
                    self.m.p.write(part + pr, 1e9f64);
                }
            }
        } else if !self.tough.is_empty() {
            // Only parts still on the vessel: others may be gone from memory.
            for &part in parts {
                if let Some((oc, og, op)) = self.tough.get(&part).copied() {
                    self.m.p.write(part + c, oc);
                    self.m.p.write(part + g, og);
                    self.m.p.write(part + pr, op);
                }
            }
            self.tough.clear();
        }
    }

    /// Fills resources picked by `pick` to their maximum.
    fn refill(&mut self, resources: &[(u64, String)], pick: impl Fn(&str) -> bool) {
        let (Some(ao), Some(mo)) = (self.f("PartResource", "amount"), self.f("PartResource", "maxAmount")) else { return };
        for (res, name) in resources {
            if !pick(name) {
                continue;
            }
            let (Some(amount), Some(max)) = (self.m.p.read::<f64>(res + ao), self.m.p.read::<f64>(res + mo)) else { continue };
            if max > 0.0 && max.is_finite() && amount < max {
                self.m.p.write(res + ao, max);
            }
        }
    }

    pub fn tick(&mut self, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Result<bool, String> {
        if self.m.class("", "HighLogic").is_none() {
            return Err("Game closed".into());
        }
        // Still on the loading screen until HighLogic exists.
        if self.static_obj("HighLogic", "fetch").is_none() {
            return Ok(false);
        }
        let none = HashMap::new();
        let v = values.unwrap_or(&none);
        let on = |k: &str| v.get(k).is_some_and(|x| *x != 0.0);

        // The game's own cheats.
        let invincible = on("invincible-craft");
        self.cheat("NoCrashDamage", invincible);
        self.cheat("UnbreakableJoints", invincible);
        self.cheat("IgnoreMaxTemperature", on("no-overheating") || invincible);
        self.cheat("InfiniteElectricity", on("unlimited-flight-resources"));
        self.cheat("AllowPartClipping", on("part-clipping"));
        self.cheat("NonStrictAttachmentOrientation", on("free-attachment"));
        self.cheat("BiomesVisible", on("show-biomes"));
        self.cheat("IgnoreKerbalInventoryLimits", on("unlimited-inventory"));
        self.cheat("IgnoreEVAConstructionMassLimit", on("unlimited-inventory"));
        self.cheat("IgnoreAgencyMindsetOnContracts", on("any-contract"));

        // Gravity, as a percentage of normal.
        if let Some(a) = self.static_addr("PhysicsGlobals", "GraviticForceMultiplier") {
            let want = v.get("gravity").copied().unwrap_or(100.0) / 100.0;
            if self.m.p.read::<f64>(a).is_some_and(|g| (g - want).abs() > 1e-6) {
                self.m.p.write(a, want);
            }
        }

        // The vessel you're flying: toughness and refills.
        let (fuel, mono, flight) = (on("unlimited-fuel"), on("unlimited-monopropellant"), on("unlimited-flight-resources"));
        let parts = if invincible || !self.tough.is_empty() || fuel || mono || flight { self.vessel_parts() } else { vec![] };
        self.toughen(&parts, invincible);
        if fuel || mono || flight {
            let res = self.vessel_resources(&parts);
            self.refill(&res, |n| {
                (fuel && n != "MonoPropellant" && (FUELS.contains(&n) || n.contains("Fuel") || n.contains("Propellant")))
                    || (mono && n == "MonoPropellant")
                    || (flight && FLIGHT.contains(&n))
            });
        }

        // Career: funds, science and reputation (absent in sandbox).
        let funding = self.static_obj("Funding", "Instance");
        if let (Some(obj), Some(o)) = (funding, self.f("Funding", "funds")) {
            let addr = obj + o;
            if on("unlimited-funds") {
                if self.funds_orig.map_or(true, |(owner, _)| owner != obj) {
                    self.funds_orig = self.m.p.read::<f64>(addr).map(|f| (obj, f));
                }
                if self.m.p.read::<f64>(addr).is_some_and(|f| f < FUNDS) {
                    self.m.p.write(addr, FUNDS);
                }
            } else if let Some((owner, f)) = self.funds_orig.take() {
                if owner == obj {
                    self.m.p.write(addr, f);
                }
            }
            if actions.iter().any(|a| a == "add-100-000-funds") {
                let f = self.m.p.read::<f64>(addr).unwrap_or(0.0);
                self.m.p.write(addr, f + 100_000.0);
                if let Some((_, base)) = self.funds_orig.as_mut() {
                    *base += 100_000.0;
                }
            }
        }
        let rnd = self.static_obj("ResearchAndDevelopment", "Instance");
        if let (Some(obj), Some(o)) = (rnd, self.f("ResearchAndDevelopment", "science")) {
            let addr = obj + o;
            if on("unlimited-research-points") {
                if self.science_orig.map_or(true, |(owner, _)| owner != obj) {
                    self.science_orig = self.m.p.read::<f32>(addr).map(|s| (obj, s));
                }
                if self.m.p.read::<f32>(addr).is_some_and(|s| s < SCIENCE) {
                    self.m.p.write(addr, SCIENCE);
                }
            } else if let Some((owner, s)) = self.science_orig.take() {
                if owner == obj {
                    self.m.p.write(addr, s);
                }
            }
            if actions.iter().any(|a| a == "add-1-000-science") {
                let s = self.m.p.read::<f32>(addr).unwrap_or(0.0);
                self.m.p.write(addr, s + 1000.0);
                if let Some((_, base)) = self.science_orig.as_mut() {
                    *base += 1000.0;
                }
            }
        }
        let rep = self.static_obj("Reputation", "Instance");
        if let (Some(obj), Some(o)) = (rep, self.f("Reputation", "rep")) {
            let addr = obj + o;
            // The game caps reputation at RepRange (1000).
            let max = self.static_addr("Reputation", "RepRange").and_then(|a| self.m.p.read::<f32>(a)).filter(|m| *m > 0.0).unwrap_or(1000.0);
            if on("max-reputation") {
                if self.rep_orig.map_or(true, |(owner, _)| owner != obj) {
                    self.rep_orig = self.m.p.read::<f32>(addr).map(|r| (obj, r));
                }
                if self.m.p.read::<f32>(addr).is_some_and(|r| r < max) {
                    self.m.p.write(addr, max);
                }
            } else if let Some((owner, r)) = self.rep_orig.take() {
                if owner == obj {
                    self.m.p.write(addr, r);
                }
            }
            if actions.iter().any(|a| a == "add-100-reputation") {
                let r = self.m.p.read::<f32>(addr).unwrap_or(0.0);
                self.m.p.write(addr, (r + 100.0).min(max));
                if let Some((_, base)) = self.rep_orig.as_mut() {
                    *base = (*base + 100.0).min(max);
                }
            }
        }
        Ok(true)
    }
}

/// Dev aid (`hotswap --ksp-peek`): prints what the trainer sees, without changing anything.
pub fn peek(m: Mono) {
    let mut s = Session::new(m);
    for c in ["NoCrashDamage", "UnbreakableJoints", "IgnoreMaxTemperature", "InfiniteElectricity", "AllowPartClipping", "BiomesVisible"] {
        let a = s.static_addr("CheatOptions", c);
        println!("CheatOptions.{c} at {a:X?} = {:?}", a.and_then(|a| s.m.p.read::<u8>(a)));
    }
    let g = s.static_addr("PhysicsGlobals", "GraviticForceMultiplier");
    println!("gravity at {g:X?} = {:?}", g.and_then(|a| s.m.p.read::<f64>(a)));
    println!("HighLogic.fetch = {:X?}", s.static_obj("HighLogic", "fetch"));
    println!("FlightGlobals._fetch = {:X?}", s.static_obj("FlightGlobals", "_fetch"));
    let f = s.static_obj("Funding", "Instance");
    println!("Funding = {f:X?} funds = {:?}", f.zip(s.f("Funding", "funds")).and_then(|(o, x)| s.m.p.read::<f64>(o + x)));
    let r = s.static_obj("ResearchAndDevelopment", "Instance");
    println!("R&D = {r:X?} science = {:?}", r.zip(s.f("ResearchAndDevelopment", "science")).and_then(|(o, x)| s.m.p.read::<f32>(o + x)));
    let p = s.static_obj("Reputation", "Instance");
    println!("Reputation = {p:X?} rep = {:?}", p.zip(s.f("Reputation", "rep")).and_then(|(o, x)| s.m.p.read::<f32>(o + x)));
    println!("offsets: parts {:?} _resources {:?} dict {:?} list {:?} name {:?} amount {:?} max {:?}",
        s.f("Vessel", "parts"), s.f("Part", "_resources"), s.f("PartResourceList", "dict"), s.f("DictionaryValueList`2", "list"),
        s.f("PartResource", "resourceName"), s.f("PartResource", "amount"), s.f("PartResource", "maxAmount"));
    let parts = s.vessel_parts();
    let res = s.vessel_resources(&parts);
    let (ao, mo) = (s.f("PartResource", "amount").unwrap_or(0), s.f("PartResource", "maxAmount").unwrap_or(0));
    for (r, n) in res.iter().take(40) {
        println!("  {n}: {:?} / {:?}", s.m.p.read::<f64>(r + ao), s.m.p.read::<f64>(r + mo));
    }
    println!("{} resources on the active vessel", res.len());
}
