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
            let mut last_attach: Option<Instant> = None;
            let mut last = Status { game: None, connected: false, message: String::new() };
            loop {
                let (game, paused, values, actions) = {
                    let mut w = me.0.lock().unwrap();
                    (w.game.clone(), w.paused, w.values.clone(), std::mem::take(&mut w.actions))
                };
                let status = match game.as_deref() {
                    Some(SATISFACTORY) => {
                        // Attaching is the expensive part; try at most every 3 seconds.
                        if session.is_none() && last_attach.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                            last_attach = Some(Instant::now());
                            session = pid_of(SATISFACTORY).and_then(|pid| Ue::attach(pid).ok()).map(Session::new);
                        }
                        match session.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }, &actions)) {
                            Some(Ok(true)) => Status { game: game.clone(), connected: true, message: "Connected".into() },
                            Some(Ok(false)) => Status { game: game.clone(), connected: false, message: "Waiting for the game to load".into() },
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

/// Dev aid (`hotswap --trainer-test`): shows what the trainer finds and whether a write sticks.
pub fn debug(pid: u32) {
    let ue = match Ue::attach(pid) {
        Ok(u) => u,
        Err(e) => return println!("attach failed: {e}"),
    };
    let mut s = Session::new(ue);
    for p in s.ue.find_by_class("Char_Player_C") {
        let ctrl = s.off(p, "Controller").ok().and_then(|o| s.ue.p.ptr(p + o));
        println!("player 0x{p:X} {} controller={ctrl:X?}", s.ue.name_of(p).unwrap_or_default());
    }
    if let Err(e) = s.resolve() {
        return println!("resolve failed: {e}");
    }
    let m = s.t.movement;
    println!("chosen player 0x{:X}, movement 0x{m:X} ({})", s.t.player, s.ue.name_of(m).unwrap_or_default());
    for name in ["MaxWalkSpeed", "mMaxSprintSpeed", "JumpZVelocity", "GravityScale"] {
        let o = s.off(m, name).unwrap_or(0);
        println!("  {name} @+0x{o:X} = {:?}", s.ue.p.read::<f32>(m + o));
    }
    let o = s.off(m, "JumpZVelocity").unwrap_or(0);
    let before = s.ue.p.read::<f32>(m + o).unwrap_or(0.0);
    println!("write JumpZVelocity {before} -> {} : ok={}", before * 3.0, s.ue.p.write(m + o, before * 3.0));
    thread::sleep(Duration::from_secs(1));
    println!("1s later: {:?}", s.ue.p.read::<f32>(m + o));
    s.ue.p.write(m + o, before);
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
    gs: u64,
    /// The save's own value of each cheat switch, restored when a mod is turned off.
    orig_bools: HashMap<&'static str, u8>,
    /// Infinite items: inventory slot -> (item class, highest count seen).
    held: HashMap<u64, (u64, i32)>,
    held_for: u64,
    fuel_max: HashMap<u64, f32>,
}

impl Session {
    fn new(ue: Ue) -> Session {
        Session { ue, t: Targets::default(), resolved_at: None, offsets: HashMap::new(), base: HashMap::new(), base_for: 0, gs: 0, orig_bools: HashMap::new(), held: HashMap::new(), held_for: 0, fuel_max: HashMap::new() }
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

    /// The world's game state (holds the built-in cheat switches). Found once, re-found if it goes away.
    fn game_state(&mut self) -> Option<u64> {
        let ok = |s: &Self, gs: u64| s.ue.class_of(gs).and_then(|c| s.ue.name_of(c)).is_some_and(|n| n.contains("GameState"));
        if self.gs != 0 && ok(self, self.gs) {
            return Some(self.gs);
        }
        self.gs = ["BP_GameState_C", "FGGameState"].iter().find_map(|c| self.ue.find_by_class(c).first().copied()).unwrap_or(0);
        (self.gs != 0).then_some(self.gs)
    }

    /// Infinite items: stacks in the player's inventory never go down, and used-up stacks come back.
    fn hold_inventory(&mut self) -> Result<(), String> {
        const STACK: u64 = 0x30; // FInventoryStack: Item { _, ItemClass @0x8, ... }, NumItems @0x28
        let player = self.t.player;
        let io = self.off(player, "mInventory")?;
        let inv = self.ue.p.ptr(player + io).ok_or("no inventory")?;
        let arr = inv + self.off(inv, "mInventoryStacks")?;
        let (data, num) = (self.ue.p.ptr(arr).unwrap_or(0), self.ue.p.read::<i32>(arr + 8).unwrap_or(0).clamp(0, 500) as u64);
        if self.held_for != inv {
            self.held.clear();
            self.held_for = inv;
        }
        for i in 0..num {
            let slot = data + i * STACK;
            let class = self.ue.p.read::<u64>(slot + 0x8).unwrap_or(0);
            let count = self.ue.p.read::<i32>(slot + 0x28).unwrap_or(0);
            match self.held.get(&i).copied() {
                // Same item, fewer than before: top it back up.
                Some((c, max)) if c == class && count < max => {
                    self.ue.p.write(slot + 0x28, max);
                }
                // Slot was emptied by using the last item: put the item back.
                Some((c, max)) if class == 0 && count == 0 => {
                    self.ue.p.write(slot + 0x8, c);
                    self.ue.p.write(slot + 0x28, max);
                }
                _ if class != 0 && count > 0 => {
                    self.held.insert(i, (class, count.max(self.held.get(&i).filter(|h| h.0 == class).map_or(0, |h| h.1))));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Unlimited jetpack fuel: fuel never drops below the highest level seen.
    fn hold_jetpack_fuel(&mut self) -> Result<(), String> {
        for jp in self.ue.find_by_class("Equip_JetPack_C") {
            let addr = jp + self.off(jp, "mCurrentFuel")?;
            let cur = self.ue.p.read::<f32>(addr).unwrap_or(0.0);
            let max = self.fuel_max.entry(jp).or_insert(cur);
            if cur > *max {
                *max = cur;
            } else if cur < *max {
                self.ue.p.write(addr, *max);
            }
        }
        Ok(())
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
        Ok(())
    }

    /// Ok(true) when values were applied, Ok(false) while the world is still loading,
    /// Err only when the game is gone (the caller then drops this session).
    fn tick(&mut self, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Result<bool, String> {
        if self.ue.count() == 0 {
            return Err("Game closed".into());
        }
        if !self.still_valid() {
            // Scanning every object is heavy, so while loading only look every 2 seconds.
            if self.resolved_at.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
                return Ok(false);
            }
            self.resolved_at = Some(Instant::now());
            if self.resolve().is_err() {
                self.t = Targets::default();
                return Ok(false);
            }
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
        // The game's own cheat switches. When a mod is off, put back what the save had.
        if let Some(gs) = self.game_state() {
            for (id, name) in [
                ("easy-crafting", "mCheatNoCost"),
                ("no-power-required", "mCheatNoPower"),
                ("no-fuel-needed", "mCheatNoFuel"),
                ("turbo-production", "mCheatTurboProductionMode"),
                ("turbo-building", "mCheatTurboBuildMode"),
            ] {
                let addr = gs + self.off(gs, name)?;
                let current = self.ue.p.read::<u8>(addr).unwrap_or(0);
                let original = *self.orig_bools.entry(name).or_insert(current);
                let want = if get(id, 0.0) != 0.0 { 1 } else { original };
                if self.ue.p.read::<u8>(addr) != Some(want) {
                    self.ue.p.write(addr, want);
                }
            }
        }
        if get("infinite-items", 0.0) != 0.0 {
            self.hold_inventory()?;
        } else {
            self.held.clear();
        }
        if get("unlimited-jetpack-fuel", 0.0) != 0.0 {
            self.hold_jetpack_fuel()?;
        }
        for a in actions {
            if a == "add-1-000-coupons" && sink != 0 {
                let o = self.off(sink, "mNumResourceSinkCoupons")?;
                let n = self.ue.p.read::<i32>(sink + o).unwrap_or(0);
                self.ue.p.write(sink + o, n.saturating_add(1000));
            }
        }
        Ok(true)
    }
}
