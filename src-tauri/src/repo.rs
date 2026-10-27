//! R.E.P.O. (Steam app 3241660), a Unity game on Mono (see mono.rs). Works for the local player
//! in single-player and private lobbies.

use crate::{mono::Mono, trainer::log};
use std::collections::HashMap;

pub const REPO: &str = "3241660";

pub struct Session {
    m: Mono,
    avatar: u64,
    health: u64,
    controller: u64,
    off: HashMap<&'static str, u64>,
    /// God mode switch: the value before we turned it on.
    god_orig: Option<u8>,
    /// Other game switches we turned on: address -> the value before.
    switched: HashMap<u64, u8>,
    /// The grabber's own strengths (with your upgrades) before scaling: field -> value.
    grab_orig: HashMap<&'static str, f32>,
    grabber: u64,
    /// Enemy radar data for the overlay, when the radar is on.
    pub radar: Option<serde_json::Value>,
}

impl Session {
    pub fn new(m: Mono) -> Session {
        Session { m, avatar: 0, health: 0, controller: 0, off: HashMap::new(), god_orig: None, switched: HashMap::new(), grab_orig: HashMap::new(), grabber: 0, radar: None }
    }

    fn f(&mut self, class: &str, field: &'static str) -> Option<u64> {
        if let Some(&o) = self.off.get(field) {
            return Some(o);
        }
        let c = self.m.class("", class)?;
        let o = self.m.field(c, field)? as u64;
        self.off.insert(field, o);
        Some(o)
    }
    fn static_obj(&mut self, class: &str, field: &str) -> Option<u64> {
        let c = self.m.class("", class)?;
        let a = self.m.static_addr(c, field)?;
        self.m.p.ptr(a)
    }
    fn read_f32(&mut self, obj: u64, class: &str, field: &'static str) -> Option<f32> {
        let o = self.f(class, field)?;
        self.m.p.read::<f32>(obj + o)
    }
    fn write_f32(&mut self, obj: u64, class: &str, field: &'static str, v: f32) {
        if let Some(o) = self.f(class, field) {
            if self.m.p.read::<f32>(obj + o).is_some_and(|c| (c - v).abs() > 1e-4) {
                self.m.p.write(obj + o, v);
            }
        }
    }

    /// Keeps a game bool on while `on`, and puts back its old value when turned off.
    fn switch(&mut self, obj: u64, class: &str, field: &'static str, on: bool) {
        let Some(o) = self.f(class, field) else { return };
        let addr = obj + o;
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

    /// You (position and facing) and every spawned enemy with its name and position.
    fn scan_enemies(&mut self, avatar: u64) -> Option<serde_json::Value> {
        let to = self.f("PlayerAvatar", "playerTransform")?;
        let (me, rot) = self.m.transform_world(self.m.p.ptr(avatar + to)?)?;
        // Facing: the body's forward (0, 0, 1) rotated, as a compass angle (0 = +z).
        let [x, y, z, w] = rot;
        let (fx, fz) = (2.0 * (x * z + w * y), 1.0 - 2.0 * (x * x + y * y));
        let yaw = fx.atan2(fz).to_degrees();
        let dir = self.static_obj("EnemyDirector", "instance")?;
        let lo = self.f("EnemyDirector", "enemiesSpawned")?;
        let list = self.m.p.ptr(dir + lo)?;
        let (items, n) = (self.m.p.ptr(list + 0x10)?, self.m.p.read::<i32>(list + 0x18)?);
        let (eo, no, so) = (self.f("EnemyParent", "Enemy")?, self.f("EnemyParent", "enemyName")?, self.f("EnemyParent", "Spawned")?);
        let ct = self.f("Enemy", "CenterTransform")?;
        let mut enemies = Vec::new();
        for i in 0..n.clamp(0, 64) as u64 {
            let Some(parent) = self.m.p.ptr(items + 0x20 + i * 8) else { continue };
            if self.m.p.read::<u8>(parent + so) != Some(1) {
                continue;
            }
            let Some(pos) = self.m.p.ptr(parent + eo).and_then(|e| self.m.p.ptr(e + ct)).and_then(|t| self.m.transform_pos(t)) else { continue };
            let name = self.m.p.ptr(parent + no).map(|s| self.m.string(s)).unwrap_or_default();
            enemies.push(serde_json::json!({ "name": name, "x": pos[0], "y": pos[1], "z": pos[2] }));
        }
        Some(serde_json::json!({ "me": { "x": me[0], "y": me[1], "z": me[2], "yaw": yaw }, "enemies": enemies }))
    }

    pub fn tick(&mut self, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Result<bool, String> {
        if self.m.class("", "PlayerAvatar").is_none() {
            return Err("Game closed".into());
        }
        // The local player (null in menus and while a level loads).
        let Some(avatar) = self.static_obj("PlayerAvatar", "instance").filter(|&a| a != 0) else { return Ok(false) };
        if avatar != self.avatar {
            let ho = self.f("PlayerAvatar", "playerHealth").unwrap_or(0);
            self.health = self.m.p.ptr(avatar + ho).unwrap_or(0);
            self.controller = self.static_obj("PlayerController", "instance").unwrap_or(0);
            self.avatar = avatar;
            self.god_orig = None;
            self.switched.clear();
            log(&format!("repo: player 0x{avatar:X} health 0x{:X} controller 0x{:X}", self.health, self.controller));
        }
        let none = HashMap::new();
        let v = values.unwrap_or(&none);
        let on = |k: &str| v.get(k).is_some_and(|x| *x != 0.0);

        // Health: the game's own god mode switch, and health kept at max.
        if self.health != 0 {
            if let Some(go) = self.f("PlayerHealth", "godMode") {
                let addr = self.health + go;
                if on("infinite-health") {
                    if self.god_orig.is_none() {
                        self.god_orig = self.m.p.read::<u8>(addr);
                    }
                    self.m.p.write(addr, 1u8);
                    if let (Some(h), Some(mx)) = (self.f("PlayerHealth", "health"), self.f("PlayerHealth", "maxHealth")) {
                        let max = self.m.p.read::<i32>(self.health + mx).unwrap_or(100);
                        if self.m.p.read::<i32>(self.health + h).is_some_and(|c| c < max) {
                            self.m.p.write(self.health + h, max);
                        }
                    }
                } else if let Some(o) = self.god_orig.take() {
                    self.m.p.write(addr, o);
                }
            }
        }
        let pc = self.controller;
        if pc != 0 {
            // Stamina: the game's own infinite energy switch (no drain), plus a refill.
            self.switch(pc, "PlayerController", "DebugEnergy", on("infinite-stamina"));
            if on("infinite-jumps") {
                if let Some(o) = self.f("PlayerController", "JumpExtraCurrent") {
                    if self.m.p.read::<i32>(pc + o).is_some_and(|j| j < 50) {
                        self.m.p.write(pc + o, 99i32);
                    }
                }
            }
            if on("infinite-stamina") {
                if let Some(max) = self.read_f32(pc, "PlayerController", "EnergyStart").filter(|m| *m > 0.0) {
                    if self.read_f32(pc, "PlayerController", "EnergyCurrent").is_some_and(|c| c < max) {
                        self.write_f32(pc, "PlayerController", "EnergyCurrent", max);
                    }
                }
            }
            // Speeds as a percentage of the game's own originals (which it keeps itself).
            let f = (v.get("movement-speed").copied().unwrap_or(100.0) / 100.0) as f32;
            for (field, orig) in [("MoveSpeed", "playerOriginalMoveSpeed"), ("SprintSpeed", "playerOriginalSprintSpeed"), ("CrouchSpeed", "playerOriginalCrouchSpeed")] {
                if let Some(base) = self.read_f32(pc, "PlayerController", orig).filter(|b| *b > 0.0) {
                    self.write_f32(pc, "PlayerController", field, base * f);
                }
            }
        }
        // Carrying: grab and throw strength as a percentage of your own (upgrades included).
        if let Some(g) = self.static_obj("PhysGrabber", "instance").filter(|&g| g != 0) {
            if g != self.grabber {
                self.grabber = g;
                self.grab_orig.clear();
            }
            for (key, field) in [("grab-strength", "grabStrength"), ("throw-strength", "throwStrength")] {
                let f = (v.get(key).copied().unwrap_or(100.0) / 100.0) as f32;
                if !self.grab_orig.contains_key(field) {
                    match self.read_f32(g, "PhysGrabber", field) {
                        Some(b) if b > 0.0 => { self.grab_orig.insert(field, b); }
                        _ => continue,
                    }
                }
                let base = self.grab_orig[field];
                // Upgrades bought mid-run raise the game's value: take that as the new base.
                if let Some(cur) = self.read_f32(g, "PhysGrabber", field) {
                    let last = base * f;
                    if (cur - last).abs() > 0.01 && (cur - base).abs() > 0.01 && f != 1.0 {
                        self.grab_orig.insert(field, cur);
                    }
                }
                let base = self.grab_orig[field];
                self.write_f32(g, "PhysGrabber", field, base * f);
            }
        }
        self.radar = if on("enemy-radar") { self.scan_enemies(avatar) } else { None };
        // Round: infinite battery (the game's debug switch) and completing every extraction point.
        if let Some(rd) = self.static_obj("RoundDirector", "instance").filter(|&r| r != 0) {
            self.switch(rd, "RoundDirector", "debugInfiniteBattery", on("infinite-battery"));
            if actions.iter().any(|a| a == "complete-extractions") {
                if let (Some(n), Some(done), Some(all)) = (self.f("RoundDirector", "extractionPoints"), self.f("RoundDirector", "extractionPointsCompleted"), self.f("RoundDirector", "allExtractionPointsCompleted")) {
                    let total = self.m.p.read::<i32>(rd + n).unwrap_or(0);
                    self.m.p.write(rd + done, total);
                    self.m.p.write(rd + all, 1u8);
                    log(&format!("repo: marked {total} extraction points complete"));
                }
            }
        }
        Ok(true)
    }
}
