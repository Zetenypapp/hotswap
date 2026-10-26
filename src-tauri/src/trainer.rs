//! Applies mod values to a running game. A background thread attaches when the
//! game is running, re-finds the player when needed, and writes the wanted values
//! twice a second (so they stick after respawns or the game resetting them).
//! Currently supports Satisfactory (Steam app 526870).

use crate::ue::objects::Ue;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

pub const SATISFACTORY: &str = "526870";

#[derive(Default)]
struct Wanted {
    game: Option<String>,
    paused: bool,
    values: HashMap<String, f64>,
    actions: Vec<String>,
}

#[derive(Clone, Default)]
pub struct Trainer(Arc<Mutex<Wanted>>);

#[derive(Serialize, Clone, PartialEq)]
struct Status {
    game: Option<String>,
    connected: bool,
    message: String,
}

impl Trainer {
    pub fn set(&self, game: Option<String>, paused: bool, values: HashMap<String, f64>) {
        let mut w = self.0.lock().unwrap();
        w.game = game;
        w.paused = paused;
        w.values = values;
    }

    pub fn action(&self, id: String) {
        self.0.lock().unwrap().actions.push(id);
    }

    pub fn start(&self, app: AppHandle, pid_of: impl Fn(&str) -> Option<u32> + Send + 'static) {
        let me = self.clone();
        thread::spawn(move || {
            let mut session: Option<Session> = None;
            let mut last = Status { game: None, connected: false, message: String::new() };
            loop {
                let (game, paused, values, actions) = {
                    let mut w = me.0.lock().unwrap();
                    (w.game.clone(), w.paused, w.values.clone(), std::mem::take(&mut w.actions))
                };
                let status = match game.as_deref() {
                    Some(SATISFACTORY) => {
                        if session.is_none() {
                            session = pid_of(SATISFACTORY).and_then(|pid| Ue::attach(pid).ok()).map(Session::new);
                        }
                        match session.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }, &actions)) {
                            Some(Ok(())) => Status { game: game.clone(), connected: true, message: "Connected".into() },
                            Some(Err(e)) => {
                                session = None;
                                Status { game: game.clone(), connected: false, message: e }
                            }
                            None => Status { game: game.clone(), connected: false, message: "Waiting for the game to load".into() },
                        }
                    }
                    _ => {
                        session = None;
                        Status { game: game.clone(), connected: false, message: String::new() }
                    }
                };
                if status != last {
                    let _ = app.emit("trainer-status", &status);
                    last = status;
                }
                thread::sleep(Duration::from_millis(500));
            }
        });
    }
}

/// Addresses of the objects mods touch, found by class and property name.
#[derive(Default, Clone, Copy)]
struct Targets {
    player: u64,
    movement: u64,
    health: u64,
    world: u64,
    sink: u64,
}

struct Session {
    ue: Ue,
    t: Targets,
    resolved_at: Option<Instant>,
    offsets: HashMap<&'static str, u64>,
    /// Game defaults read from the movement component, so sliders are percentages of them.
    base: HashMap<&'static str, f32>,
    base_for: u64,
}

impl Session {
    fn new(ue: Ue) -> Session {
        Session { ue, t: Targets::default(), resolved_at: None, offsets: HashMap::new(), base: HashMap::new(), base_for: 0 }
    }

    fn off(&mut self, obj: u64, name: &'static str) -> Result<u64, String> {
        if let Some(&o) = self.offsets.get(name) {
            return Ok(o);
        }
        let cls = self.ue.class_of(obj).ok_or("object went away")?;
        let p = self.ue.prop(cls, name).ok_or(format!("{name} not found"))?;
        self.offsets.insert(name, p.offset as u64);
        Ok(p.offset as u64)
    }

    fn still_valid(&self) -> bool {
        self.t.player != 0 && self.ue.name_of(self.t.player).is_some_and(|n| n.starts_with("Char_Player_C"))
    }

    fn resolve(&mut self) -> Result<(), String> {
        let players = self.ue.find_by_class("Char_Player_C");
        if players.is_empty() {
            return Err("Waiting for the game to load".into());
        }
        // The player you control is the one with a controller.
        let probe = players[0];
        let ctrl = self.off(probe, "Controller")?;
        let player = players.iter().copied().find(|&p| self.ue.p.ptr(p + ctrl).is_some()).unwrap_or(probe);
        let (mo, ho) = (self.off(player, "CharacterMovement")?, self.off(player, "mHealthComponent")?);
        let movement = self.ue.p.ptr(player + mo).ok_or("no movement component")?;
        let health = self.ue.p.ptr(player + ho).ok_or("no health component")?;
        let world = self.ue.find_by_class("FGWorldSettings").first().copied().unwrap_or(0);
        let sink = self.ue.find_by_class("FGResourceSinkSubsystem").first().copied().unwrap_or(0);
        self.t = Targets { player, movement, health, world, sink };
        self.resolved_at = Some(Instant::now());
        Ok(())
    }

    fn tick(&mut self, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Result<(), String> {
        if self.ue.count() == 0 {
            return Err("Game closed".into());
        }
        if !self.still_valid() || self.resolved_at.map_or(true, |t| t.elapsed() > Duration::from_secs(20)) {
            self.resolve()?;
        }
        let Targets { movement, health, world, sink, .. } = self.t;

        // Remember the game's own movement values once per movement component.
        const MOVE: [&str; 4] = ["MaxWalkSpeed", "mMaxSprintSpeed", "JumpZVelocity", "GravityScale"];
        if self.base_for != movement {
            self.base.clear();
            for name in MOVE {
                let o = self.off(movement, name)?;
                self.base.insert(name, self.ue.p.read::<f32>(movement + o).unwrap_or(0.0));
            }
            self.base_for = movement;
        }

        // Paused (no values): every mod reads as its default, which puts the game's own values back.
        let none = HashMap::new();
        let v = values.unwrap_or(&none);
        let get = |k: &str, d: f64| v.get(k).copied().unwrap_or(d);

        let speed = get("walking-speed", 100.0) / 100.0 * if get("super-speed", 0.0) != 0.0 { 3.0 } else { 1.0 };
        let scaled = [
            ("MaxWalkSpeed", speed),
            ("mMaxSprintSpeed", speed),
            ("JumpZVelocity", get("jump-height", 100.0) / 100.0),
            ("GravityScale", get("gravity-strength", 100.0) / 100.0),
        ];
        for (name, factor) in scaled {
            let o = self.off(movement, name)?;
            self.ue.p.write(movement + o, (self.base[name] as f64 * factor) as f32);
        }
        if world != 0 {
            let o = self.off(world, "TimeDilation")?;
            self.ue.p.write(world + o, get("game-speed", 1.0).clamp(0.1, 5.0) as f32);
        }
        if get("infinite-health", 0.0) != 0.0 || get("no-fall-damage", 0.0) != 0.0 {
            let max = self.off(health, "mMaxHealth")?;
            let cur = self.off(health, "mCurrentHealth")?;
            if let Some(m) = self.ue.p.read::<f32>(health + max) {
                self.ue.p.write(health + cur, m);
            }
        }
        for a in actions {
            if a == "add-1-000-coupons" && sink != 0 {
                let o = self.off(sink, "mNumResourceSinkCoupons")?;
                let n = self.ue.p.read::<i32>(sink + o).unwrap_or(0);
                self.ue.p.write(sink + o, n.saturating_add(1000));
            }
        }
        Ok(())
    }
}
