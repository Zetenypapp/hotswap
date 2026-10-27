//! Subnautica 2 (Steam app 1962700). The game ships without debug symbols, so it's attached by
//! scanning (see ue::scan). Player stats are Gameplay Ability System attributes on the character
//! the local player controller possesses: each is an FGameplayAttributeData (base value at +8,
//! current value at +0xC), and both are written so the game's own logic sees the new value.

use crate::{trainer::log, ue::objects::Ue};
use std::collections::HashMap;

pub const SUBNAUTICA2: &str = "1962700";
pub const MODULE: &str = "Subnautica2-Win64-Shipping";

pub struct Session {
    ue: Ue,
    controller: u64,
    pawn: u64,
    /// Attribute set class name (e.g. "UWEHealthAttributeSet") -> the player's instance.
    sets: HashMap<String, u64>,
    offs: HashMap<(String, &'static str), Option<u64>>,
    /// The game's own value of every attribute we changed, put back when the mod is turned off.
    orig: HashMap<(&'static str, &'static str), f32>,
    /// The character movement component and its own speed values before we scaled them.
    movement: u64,
    move_orig: HashMap<&'static str, f32>,
}

impl Session {
    pub fn new(ue: Ue) -> Session {
        Session { ue, controller: 0, pawn: 0, sets: HashMap::new(), offs: HashMap::new(), orig: HashMap::new(), movement: 0, move_orig: HashMap::new() }
    }

    fn prop_off(&mut self, obj: u64, name: &'static str) -> Option<u64> {
        let cls = self.ue.class_of(obj)?;
        let cname = self.ue.name_of(cls)?;
        let ue = &self.ue;
        *self.offs.entry((cname, name)).or_insert_with(|| ue.prop(cls, name).map(|p| p.offset as u64))
    }

    /// The character the local player controls right now.
    fn current_pawn(&mut self) -> Option<u64> {
        if self.controller == 0 || self.ue.name_of(self.controller).is_none_or(|n| !n.starts_with("BP_SN2PlayerController_C")) {
            self.controller = self.ue.find_by_classes(|n| n == "BP_SN2PlayerController_C").first().copied()?;
        }
        let o = self.prop_off(self.controller, "Pawn")?;
        self.ue.p.ptr(self.controller + o)
    }

    fn resolve(&mut self, pawn: u64) {
        self.pawn = pawn;
        self.sets = self
            .ue
            .find_by_classes(|n| n.starts_with("UWE") && n.ends_with("AttributeSet"))
            .into_iter()
            .filter(|&s| self.ue.outer_of(s) == Some(pawn))
            .filter_map(|s| Some((self.ue.class_of(s).and_then(|c| self.ue.name_of(c))?, s)))
            .collect();
        log(&format!("sn2: player 0x{pawn:X}, {} attribute sets", self.sets.len()));
    }

    /// Address of attribute `name` in the player's `set`.
    fn attr(&mut self, set: &'static str, name: &'static str) -> Option<u64> {
        let s = *self.sets.get(set)?;
        Some(s + self.prop_off(s, name)?)
    }
    fn get(&mut self, set: &'static str, name: &'static str) -> Option<f32> {
        let a = self.attr(set, name)?;
        self.ue.p.read::<f32>(a + 12)
    }
    fn put(&mut self, set: &'static str, name: &'static str, v: f32) {
        if let Some(a) = self.attr(set, name) {
            if self.ue.p.read::<f32>(a + 12) != Some(v) {
                self.ue.p.write(a + 8, v);
                self.ue.p.write(a + 12, v);
            }
        }
    }
    /// Keeps an attribute at `v` while `on`, remembering the game's value; puts it back when off.
    fn hold(&mut self, on: bool, set: &'static str, name: &'static str, v: f32) {
        if on {
            if !self.orig.contains_key(&(set, name)) {
                let Some(cur) = self.get(set, name) else { return };
                self.orig.insert((set, name), cur);
            }
            self.put(set, name, v);
        } else if let Some(o) = self.orig.remove(&(set, name)) {
            self.put(set, name, o);
        }
    }
    /// Keeps a resource (health, oxygen...) topped up to its maximum.
    fn fill(&mut self, set: &'static str, name: &'static str, max: &'static str) {
        if let Some(m) = self.get(set, max).filter(|m| *m > 0.0) {
            if self.get(set, name).is_some_and(|c| c < m) {
                self.put(set, name, m);
            }
        }
    }

    /// Ok(true) when values were applied, Ok(false) while loading; Err when the game is gone.
    pub fn tick(&mut self, values: Option<&HashMap<String, f64>>) -> Result<bool, String> {
        if self.ue.count() == 0 {
            return Err("Game closed".into());
        }
        let Some(pawn) = self.current_pawn() else { return Ok(false) };
        if pawn != self.pawn || self.sets.is_empty() {
            // New character (loaded a save, respawned): its attributes are new objects.
            self.orig.clear();
            self.move_orig.clear();
            self.resolve(pawn);
            self.movement = self.prop_off(pawn, "CharacterMovement").and_then(|o| self.ue.p.ptr(pawn + o)).unwrap_or(0);
            if self.sets.is_empty() {
                return Ok(false);
            }
        }
        let none = HashMap::new();
        let v = values.unwrap_or(&none);
        let on = |k: &str| v.get(k).is_some_and(|x| *x != 0.0);
        let pct = |k: &str| (v.get(k).copied().unwrap_or(100.0) / 100.0) as f32;

        const H: &str = "UWEHealthAttributeSet";
        const S: &str = "UWESurvivalAttributeSet";
        const M: &str = "UWEMovementAttributeSet";
        if on("infinite-health") {
            self.fill(H, "Health", "MaxHealth");
        }
        self.hold(on("infinite-health"), H, "DamageMultiplier", 0.0);
        if on("infinite-oxygen") {
            self.fill(S, "Oxygen", "MaxOxygen");
        }
        if on("no-hunger") {
            self.fill(S, "Food", "MaxFood");
        }
        if on("no-thirst") {
            self.fill(S, "Water", "MaxWater");
        }
        let temp = on("no-temperature-damage");
        self.hold(temp, H, "DamageMultiplierCold", 0.0);
        self.hold(temp, H, "DamageMultiplierHeat", 0.0);
        let rad = on("no-radiation");
        self.hold(rad, H, "DamageMultiplierRadiation", 0.0);
        if rad {
            self.put(H, "Radiation", 0.0);
        }
        self.hold(on("free-dash"), M, "DashOxygenCost", 0.0);
        // Speeds are percentages of the game's own value. The movement component has its own copy of
        // each speed that the attribute only updates when the game changes it, so scale both.
        let (walk, swim) = (pct("walk-speed"), pct("swim-speed"));
        if self.movement != 0 {
            for (name, f) in [("MaxWalkSpeed", walk), ("MaxSwimSpeed", swim), ("MaxCustomMovementSpeed", swim)] {
                let Some(o) = self.prop_off(self.movement, name) else { continue };
                let addr = self.movement + o;
                let Some(cur) = self.ue.p.read::<f32>(addr) else { continue };
                let base = *self.move_orig.entry(name).or_insert(cur);
                let want = base * f;
                if (cur - want).abs() > 0.5 {
                    self.ue.p.write(addr, want);
                }
                // The attribute follows the same real value (the component's), so a value left
                // over from an earlier session is corrected too.
                if name != "MaxCustomMovementSpeed" {
                    self.put(M, name, want);
                }
            }
        }
        Ok(true)
    }
}
