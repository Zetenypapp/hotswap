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
    /// Map: where to teleport the player (x, y, z in game units).
    teleport: Option<[f64; 3]>,
    /// Map: the player's last known x, y, z and yaw.
    pos: Option<[f64; 4]>,
}

#[derive(Clone, Default)]
pub struct Trainer(Arc<Mutex<Wanted>>);

#[derive(Serialize, Clone, PartialEq)]
struct Status {
    game: Option<String>,
    connected: bool,
    message: String,
}

/// Appends a line to %APPDATA%\app.hotswap.desktop\trainer.log (for debugging).
pub fn log(msg: &str) {
    use std::io::Write;
    let Ok(appdata) = std::env::var("APPDATA") else { return };
    let path = std::path::Path::new(&appdata).join("app.hotswap.desktop").join("trainer.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(f, "{t} {msg}");
    }
}

impl Trainer {
    pub fn set(&self, game: Option<String>, paused: bool, values: HashMap<String, f64>) {
        log(&format!("set game={game:?} paused={paused} values={values:?}"));
        let mut w = self.0.lock().unwrap();
        w.game = game;
        w.paused = paused;
        w.values = values;
    }

    pub fn action(&self, id: String) {
        self.0.lock().unwrap().actions.push(id);
    }

    pub fn position(&self) -> Option<[f64; 4]> {
        self.0.lock().unwrap().pos
    }

    pub fn teleport(&self, to: [f64; 3]) {
        self.0.lock().unwrap().teleport = Some(to);
    }

    pub fn start(&self, app: AppHandle, pid_of: impl Fn(&str) -> Option<u32> + Send + 'static) {
        let me = self.clone();
        thread::spawn(move || {
            let mut session: Option<Session> = None;
            let mut sn2: Option<crate::sn2::Session> = None;
            let mut sn1: Option<crate::sn1::Session> = None;
            let mut repo: Option<crate::repo::Session> = None;
            let mut ksp: Option<crate::ksp::Session> = None;
            let mut extra: Option<crate::extra::Session> = None;
            let mut scrap: Option<crate::scrap::Session> = None;
            let mut last_attach: Option<Instant> = None;
            let mut last = Status { game: None, connected: false, message: String::new() };
            loop {
                let (game, paused, values, actions, teleport) = {
                    let mut w = me.0.lock().unwrap();
                    (w.game.clone(), w.paused, w.values.clone(), std::mem::take(&mut w.actions), w.teleport.take())
                };
                if let Some(s) = session.as_mut() {
                    s.teleport = teleport;
                }
                let status = match game.as_deref() {
                    Some(SATISFACTORY) => {
                        // Attaching is the expensive part; try at most every 3 seconds.
                        if session.is_none() && last_attach.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                            last_attach = Some(Instant::now());
                            session = pid_of(SATISFACTORY).and_then(|pid| Ue::attach(pid).ok()).map(Session::new);
                        }
                        let started = Instant::now();
                        // A panic here (bad memory from a game that's loading or closing) must not kill the trainer.
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            session.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }, &actions))
                        }))
                        .unwrap_or_else(|_| Some(Err("Lost track of the game, reconnecting".into())));
                        me.0.lock().unwrap().pos = session.as_ref().and_then(|s| s.pos);
                        if started.elapsed() > Duration::from_millis(400) {
                            log(&format!("slow tick: {:?}", started.elapsed()));
                        }
                        if let Some(Err(e)) = &result {
                            log(&format!("tick error: {e}"));
                        }
                        match result {
                            Some(Ok(true)) => Status { game: game.clone(), connected: true, message: "Connected".into() },
                            Some(Ok(false)) => Status { game: game.clone(), connected: false, message: "Waiting for the game to load".into() },
                            Some(Err(e)) => {
                                session = None;
                                Status { game: game.clone(), connected: false, message: e }
                            }
                            None => Status { game: game.clone(), connected: false, message: "Waiting for the game to load".into() },
                        }
                    }
                    Some(crate::sn2::SUBNAUTICA2) => {
                        session = None;
                        if sn2.is_none() && last_attach.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                            last_attach = Some(Instant::now());
                            sn2 = pid_of(crate::sn2::SUBNAUTICA2)
                                .and_then(|pid| Ue::attach_scan(pid, crate::sn2::MODULE).map_err(|e| log(&format!("sn2 attach: {e}"))).ok())
                                .map(crate::sn2::Session::new);
                        }
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            sn2.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }))
                        }))
                        .unwrap_or_else(|_| Some(Err("Lost track of the game, reconnecting".into())));
                        match result {
                            Some(Ok(true)) => Status { game: game.clone(), connected: true, message: "Connected".into() },
                            Some(Err(e)) => {
                                sn2 = None;
                                Status { game: game.clone(), connected: false, message: e }
                            }
                            _ => Status { game: game.clone(), connected: false, message: "Waiting for the game to load".into() },
                        }
                    }
                    Some(g @ (crate::sn1::SUBNAUTICA | crate::sn1::BELOW_ZERO)) => {
                        if sn1.is_none() && last_attach.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                            last_attach = Some(Instant::now());
                            sn1 = pid_of(g)
                                .and_then(|pid| crate::mono::Mono::attach(pid).map_err(|e| log(&format!("sn1 attach: {e}"))).ok())
                                .map(if g == crate::sn1::BELOW_ZERO { crate::sn1::Session::new_zero } else { crate::sn1::Session::new });
                        }
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            sn1.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }))
                        }))
                        .unwrap_or_else(|_| Some(Err("Lost track of the game, reconnecting".into())));
                        me.0.lock().unwrap().pos = sn1.as_ref().and_then(|s| s.pos);
                        match result {
                            Some(Ok(true)) => Status { game: game.clone(), connected: true, message: "Connected".into() },
                            Some(Err(e)) => {
                                sn1 = None;
                                Status { game: game.clone(), connected: false, message: e }
                            }
                            _ => Status { game: game.clone(), connected: false, message: "Waiting for the game to load".into() },
                        }
                    }
                    Some(crate::repo::REPO) => {
                        if repo.is_none() && last_attach.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                            last_attach = Some(Instant::now());
                            repo = pid_of(crate::repo::REPO)
                                .and_then(|pid| crate::mono::Mono::attach(pid).map_err(|e| log(&format!("repo attach: {e}"))).ok())
                                .map(crate::repo::Session::new);
                        }
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            repo.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }, &actions))
                        }))
                        .unwrap_or_else(|_| Some(Err("Lost track of the game, reconnecting".into())));
                        if let Some(r) = repo.as_ref().and_then(|s| s.radar.clone()) {
                            let _ = app.emit("radar", r);
                        }
                        match result {
                            Some(Ok(true)) => Status { game: game.clone(), connected: true, message: "Connected".into() },
                            Some(Err(e)) => {
                                repo = None;
                                Status { game: game.clone(), connected: false, message: e }
                            }
                            _ => Status { game: game.clone(), connected: false, message: "Waiting for the game to load".into() },
                        }
                    }
                    Some(crate::scrap::SCRAP) if crate::scrap::ENABLED => {
                        // Looking for the process is the expensive part: at most every 3 seconds.
                        if last_attach.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                            last_attach = Some(Instant::now());
                            match pid_of(crate::scrap::SCRAP) {
                                None => scrap = None, // the game closed: take the bridge out of its scripts
                                Some(pid) if scrap.is_none() => {
                                    scrap = crate::scrap::Session::attach(pid).map_err(|e| log(&format!("scrap attach: {e}"))).ok();
                                }
                                _ => {}
                            }
                        }
                        let result = scrap.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }, &actions, teleport));
                        me.0.lock().unwrap().pos = scrap.as_ref().and_then(|s| s.pos);
                        match result {
                            Some(Ok(true)) => Status { game: game.clone(), connected: true, message: "Connected".into() },
                            Some(Err(e)) => Status { game: game.clone(), connected: false, message: e },
                            Some(Ok(false)) => Status { game: game.clone(), connected: false, message: "Waiting for a survival world. Already in one? Go back to the menu and load it again.".into() },
                            None => Status { game: game.clone(), connected: false, message: "Waiting for the game to start".into() },
                        }
                    }
                    Some(g @ (crate::extra::ONI | crate::extra::MON_BAZOU | crate::extra::OCCUPY_MARS)) => {
                        if extra.is_none() && last_attach.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                            last_attach = Some(Instant::now());
                            let kind = [crate::extra::ONI, crate::extra::MON_BAZOU, crate::extra::OCCUPY_MARS].into_iter().find(|k| *k == g).unwrap();
                            extra = pid_of(g)
                                .and_then(|pid| crate::mono::Mono::attach(pid).map_err(|e| log(&format!("{g} attach: {e}"))).ok())
                                .map(|m| crate::extra::Session::new(kind, m));
                        }
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            extra.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }, &actions))
                        }))
                        .unwrap_or_else(|_| Some(Err("Lost track of the game, reconnecting".into())));
                        match result {
                            Some(Ok(true)) => Status { game: game.clone(), connected: true, message: "Connected".into() },
                            Some(Err(e)) => {
                                extra = None;
                                Status { game: game.clone(), connected: false, message: e }
                            }
                            _ => Status { game: game.clone(), connected: false, message: "Waiting for the game to load".into() },
                        }
                    }
                    Some(crate::ksp::KSP) => {
                        if ksp.is_none() && last_attach.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
                            last_attach = Some(Instant::now());
                            ksp = pid_of(crate::ksp::KSP)
                                .and_then(|pid| crate::mono::Mono::attach(pid).map_err(|e| log(&format!("ksp attach: {e}"))).ok())
                                .map(crate::ksp::Session::new);
                        }
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            ksp.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }, &actions))
                        }))
                        .unwrap_or_else(|_| Some(Err("Lost track of the game, reconnecting".into())));
                        match result {
                            Some(Ok(true)) => Status { game: game.clone(), connected: true, message: "Connected".into() },
                            Some(Err(e)) => {
                                ksp = None;
                                Status { game: game.clone(), connected: false, message: e }
                            }
                            _ => Status { game: game.clone(), connected: false, message: "Waiting for the game to load".into() },
                        }
                    }
                    _ => {
                        session = None;
                        sn2 = None;
                        sn1 = None;
                        repo = None;
                        ksp = None;
                        scrap = None;
                        extra = None;
                        me.0.lock().unwrap().pos = None;
                        Status { game: game.clone(), connected: false, message: String::new() }
                    }
                };
                if status != last {
                    log(&format!("status connected={} msg={}", status.connected, status.message));
                    let _ = app.emit("trainer-status", &status);
                    last = status;
                }
                // The radar needs smooth updates; everything else is fine twice a second.
                let fast = game.as_deref() == Some(crate::repo::REPO) && values.get("enemy-radar").is_some_and(|v| *v != 0.0) && !paused;
                thread::sleep(Duration::from_millis(if fast { 100 } else { 500 }));
            }
        });
    }
}

/// The "Save Position" spot, in the app data folder.
fn spot_file() -> Option<std::path::PathBuf> {
    Some(std::path::Path::new(&std::env::var("APPDATA").ok()?).join("app.hotswap.desktop").join("saved_spot.json"))
}

fn spot_file_load() -> Option<[f64; 3]> {
    serde_json::from_str(&std::fs::read_to_string(spot_file()?).ok()?).ok()
}

fn spot_file_save(at: [f64; 3]) {
    if let (Some(p), Ok(s)) = (spot_file(), serde_json::to_string(&at)) {
        let _ = std::fs::write(p, s);
    }
}

/// Turbo production's saved real clocks: machine position -> [current, pending, max].
fn turbo_file() -> Option<std::path::PathBuf> {
    Some(std::path::Path::new(&std::env::var("APPDATA").ok()?).join("app.hotswap.desktop").join("turbo_clocks.json"))
}

fn turbo_file_load() -> HashMap<String, [f32; 3]> {
    turbo_file().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn turbo_file_save(map: &HashMap<String, [f32; 3]>) {
    if let (Some(p), Ok(s)) = (turbo_file(), serde_json::to_string(map)) {
        let _ = std::fs::write(p, s);
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
    let _ = s.tick(Some(&HashMap::new()), &[]);
    println!("  defaults from game: {:?}", s.base);
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
    /// Turbo production: each machine's (current, pending, max) clock before we raised it.
    turbo_orig: HashMap<u64, (f32, f32, f32)>,
    turbo_at: Option<Instant>,
    turbo_cleaned: bool,
    /// God mode: each health component's real max health, to put back afterwards.
    normal_health: HashMap<u64, f32>,
    /// No fall damage: (player, fall damage curve, override curve) to put back.
    fall_curves: Option<(u64, u64, u64)>,
    gs_search: Option<Instant>,
    jetpacks: Vec<u64>,
    jetpacks_at: Option<Instant>,
    recipe_addr: HashMap<u64, u64>,
    stack_sizes: HashMap<u64, i32>,
    add_stack_fn: Option<Option<crate::ue::call::Func>>,
    /// Machines that got at least one AddStack (so the game knows their inputs changed).
    woken: std::collections::HashSet<u64>,
    recipe_cache: HashMap<u64, Vec<(u64, i32)>>,
    /// Recipes made ingredient-free: count address -> (real count, [by workbench, by machines]).
    zeroed: HashMap<u64, (i32, [bool; 2])>,
    benches: Vec<u64>,
    benches_at: Option<Instant>,
    machines: Vec<u64>,
    machines_at: Option<Instant>,
    fed_at: Option<Instant>,
    pos: Option<[f64; 4]>,
    teleport: Option<[f64; 3]>,
    /// Offset of the root component's cached world transform translation (not reflected; found by value).
    world_loc_off: Option<u64>,
    /// After a teleport: keep the player parked there until then, so the terrain can stream in.
    hold: Option<([f64; 3], Instant)>,
    /// After a map teleport, fall damage stays off until then (you may drop from high up).
    fall_guard: Option<Instant>,
}

impl Session {
    fn new(ue: Ue) -> Session {
        Session { ue, t: Targets::default(), resolved_at: None, offsets: HashMap::new(), base: HashMap::new(), base_for: 0, gs: 0, orig_bools: HashMap::new(), held: HashMap::new(), held_for: 0, fuel_max: HashMap::new(), turbo_orig: HashMap::new(), turbo_at: None, turbo_cleaned: false, normal_health: HashMap::new(), fall_curves: None, gs_search: None, jetpacks: Vec::new(), jetpacks_at: None,
            recipe_addr: HashMap::new(), stack_sizes: HashMap::new(), add_stack_fn: None, woken: Default::default(), recipe_cache: HashMap::new(), zeroed: HashMap::new(), benches: Vec::new(), benches_at: None, machines: Vec::new(), machines_at: None, fed_at: None,
            pos: None, teleport: None, world_loc_off: None, hold: None, fall_guard: None }
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
        // Not found: searching is expensive, so only retry every 10 seconds.
        if self.gs_search.is_some_and(|t| t.elapsed() < Duration::from_secs(10)) {
            return None;
        }
        self.gs_search = Some(Instant::now());
        self.gs =["BP_GameState_C", "FGGameState"].iter().find_map(|c| self.ue.find_by_class(c).first().copied()).unwrap_or(0);
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

    /// Address of a recipe's ingredient count (the `Num` of its `mIngredients` array on the default object). Cached.
    fn ingredient_count_addr(&mut self, recipe_cls: u64) -> Option<u64> {
        if let Some(&a) = self.recipe_addr.get(&recipe_cls) {
            return Some(a).filter(|&a| a != 0);
        }
        let addr = self.ue.default_object(recipe_cls)
            .and_then(|cdo| self.off(cdo, "mIngredients").ok().map(|o| cdo + o + 8))
            .unwrap_or(0);
        self.recipe_addr.insert(recipe_cls, addr);
        Some(addr).filter(|&a| a != 0)
    }

    /// Makes the recipe need no ingredients, remembering the real count. `who` is 0 = workbench, 1 = machines.
    fn zero_recipe(&mut self, recipe_cls: u64, who: usize) {
        let Some(addr) = self.ingredient_count_addr(recipe_cls) else { return };
        let entry = self.zeroed.entry(addr).or_insert_with(|| (self.ue.p.read::<i32>(addr).unwrap_or(0), [false; 2]));
        entry.1[who] = true;
        if self.ue.p.read::<i32>(addr) != Some(0) {
            self.ue.p.write(addr, 0i32);
        }
    }

    /// Gives back the real ingredient lists of recipes zeroed by `who`, unless the other mod still needs them zeroed.
    fn restore_recipes(&mut self, who: usize) {
        let mut done = Vec::new();
        for (&addr, (orig, by)) in self.zeroed.iter_mut() {
            by[who] = false;
            if !by[0] && !by[1] {
                self.ue.p.write(addr, *orig);
                done.push(addr);
            }
        }
        for a in done {
            self.zeroed.remove(&a);
        }
    }

    /// Easy crafting: the workbench's selected recipe needs no ingredients.
    fn free_workbench(&mut self) -> Result<(), String> {
        if self.benches_at.map_or(true, |t| t.elapsed() > Duration::from_secs(10)) {
            self.benches = self.ue.find_by_classes(|n| n.contains("WorkBench"));
            self.benches_at = Some(Instant::now());
        }
        for bench in self.benches.clone() {
            let Ok(ro) = self.off(bench, "mCurrentRecipe") else { continue };
            if let Some(recipe) = self.ue.p.ptr(bench + ro) {
                self.zero_recipe(recipe, 0);
            }
        }
        Ok(())
    }

    /// Ingredients of a recipe object as (item class, amount). Cached per recipe; uses the real
    /// count even if Easy Crafting has zeroed it.
    fn recipe_items(&mut self, recipe: u64) -> Vec<(u64, i32)> {
        if let Some(v) = self.recipe_cache.get(&recipe) {
            return v.clone();
        }
        let mut out = Vec::new();
        if let Ok(o) = self.off(recipe, "mIngredients") {
            let count_addr = recipe + o + 8;
            let n = self.zeroed.get(&count_addr).map(|z| z.0).or_else(|| self.ue.p.read::<i32>(count_addr)).unwrap_or(0).clamp(0, 16) as u64;
            let data = self.ue.p.ptr(recipe + o).unwrap_or(0);
            for i in 0..n {
                let item = self.ue.p.read::<u64>(data + i * 0x10).unwrap_or(0);
                let amount = self.ue.p.read::<i32>(data + i * 0x10 + 8).unwrap_or(0);
                if item != 0 && amount > 0 {
                    out.push((item, amount));
                }
            }
        }
        self.recipe_cache.insert(recipe, out.clone());
        out
    }

    /// Turbo production: run every machine at 500% clock speed. Each machine's own clock comes back when off.
    /// A machine's identity across sessions (addresses change): its rounded position.
    fn machine_key(&mut self, m: u64) -> Option<String> {
        let ro = self.off(m, "RootComponent").ok()?;
        let root = self.ue.p.ptr(m + ro)?;
        let lo = self.off(root, "RelativeLocation").ok()?;
        let [x, y, z] = [0, 8, 16].map(|i| self.ue.p.read::<f64>(root + lo + i).unwrap_or(0.0));
        Some(format!("{x:.0},{y:.0},{z:.0}"))
    }

    fn turbo_machines(&mut self, on: bool) -> Result<(), String> {
        const TURBO: f32 = 5.0;
        if !on {
            let had = !self.turbo_orig.is_empty();
            for (m, (cur, pending, max)) in std::mem::take(&mut self.turbo_orig) {
                for (name, v) in [("mMaxPotential", max), ("mCurrentPotential", cur), ("mPendingPotential", pending)] {
                    if let Ok(o) = self.off(m, name) {
                        self.ue.p.write(m + o, v);
                    }
                }
            }
            // Machines boosted before HotSwap or the game restarted: put back the clocks saved on
            // disk, matched by position. 500% is impossible in the normal game, so anything still
            // exactly there without a saved clock goes back to 100% (max 250%). Once per session.
            if !self.turbo_cleaned {
                self.turbo_cleaned = true;
                self.ensure_machines();
                let saved = turbo_file_load();
                let (mut restored, mut reset) = (0, 0);
                for m in self.machines.clone() {
                    let (Ok(c), Ok(p), Ok(x)) = (self.off(m, "mCurrentPotential"), self.off(m, "mPendingPotential"), self.off(m, "mMaxPotential")) else { continue };
                    if self.ue.p.read::<f32>(m + c) != Some(TURBO) {
                        continue;
                    }
                    let (cur, pending, max) = match self.machine_key(m).and_then(|k| saved.get(&k).copied()) {
                        Some([cur, pending, max]) => { restored += 1; (cur, pending, max) }
                        None => { reset += 1; (1.0, 1.0, 2.5) }
                    };
                    self.ue.p.write(m + x, max);
                    self.ue.p.write(m + c, cur);
                    self.ue.p.write(m + p, pending);
                }
                if restored + reset > 0 {
                    log(&format!("turbo off: restored {restored} saved clocks, reset {reset} unknown machines to 100%"));
                }
                if had || restored > 0 {
                    turbo_file_save(&HashMap::new());
                }
            }
            return Ok(());
        }
        self.turbo_cleaned = false;
        if self.turbo_at.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            return Ok(());
        }
        self.turbo_at = Some(Instant::now());
        self.ensure_machines();
        let mut saved: Option<HashMap<String, [f32; 3]>> = None;
        let mut changed = false;
        for m in self.machines.clone() {
            let (Ok(c), Ok(p), Ok(x)) = (self.off(m, "mCurrentPotential"), self.off(m, "mPendingPotential"), self.off(m, "mMaxPotential")) else { continue };
            if !self.turbo_orig.contains_key(&m) {
                // Remember the real clock on disk too, so it survives HotSwap or the game restarting.
                let file = saved.get_or_insert_with(turbo_file_load);
                let read = |o: u64| self.ue.p.read::<f32>(m + o).unwrap_or(1.0);
                let live = (read(c), read(p), read(x));
                let key = self.machine_key(m);
                let orig = match key.as_ref().and_then(|k| file.get(k)) {
                    Some(&[cur, pending, max]) => (cur, pending, max),
                    None if live.0 == TURBO => (1.0, 1.0, 2.5),
                    None => live,
                };
                if let Some(k) = key.filter(|k| !file.contains_key(k)) {
                    file.insert(k, [orig.0, orig.1, orig.2]);
                    changed = true;
                }
                self.turbo_orig.insert(m, orig);
            }
            for o in [x, c, p] {
                if self.ue.p.read::<f32>(m + o) != Some(TURBO) {
                    self.ue.p.write(m + o, TURBO);
                }
            }
        }
        if let (true, Some(file)) = (changed, &saved) {
            turbo_file_save(file);
        }
        Ok(())
    }

    /// The list of production machines, refreshed every 30 seconds.
    fn ensure_machines(&mut self) {
        if self.machines_at.map_or(true, |t| t.elapsed() > Duration::from_secs(30)) {
            const KINDS: [&str; 11] = ["Smelter", "Foundry", "Constructor", "Assembler", "Manufacturer", "Refinery", "Blender", "Packager", "HadronCollider", "QuantumEncoder", "Converter"];
            self.machines = self.ue.find_by_classes(|n| n.starts_with("Build_") && KINDS.iter().any(|k| n.contains(k)));
            self.machines_at = Some(Instant::now());
            log(&format!("machines: {}", self.machines.len()));
        }
    }

    /// Full stack size of an item (e.g. 100 for ingots, 50000 for fluids), from its descriptor. Cached.
    fn stack_size(&mut self, item_cls: u64) -> i32 {
        if let Some(&n) = self.stack_sizes.get(&item_cls) {
            return n;
        }
        let n = self.ue.default_object(item_cls)
            .and_then(|cdo| self.off(cdo, "mCachedStackSize").ok().and_then(|o| self.ue.p.read::<i32>(cdo + o)))
            .filter(|&n| n > 0)
            .unwrap_or(100);
        self.stack_sizes.insert(item_cls, n);
        n
    }

    /// The player's x, y, z (from the root component) and facing yaw.
    fn locate(&mut self) -> Option<[f64; 4]> {
        let player = self.t.player;
        let ro = self.off(player, "RootComponent").ok()?;
        let root = self.ue.p.ptr(player + ro)?;
        let loc = root + self.off(root, "RelativeLocation").ok()?;
        let rot = root + self.off(root, "RelativeRotation").ok()?;
        let [x, y, z] = [0, 8, 16].map(|o| self.ue.p.read::<f64>(loc + o).unwrap_or(f64::NAN));
        let yaw = self.ue.p.read::<f64>(rot + 8)?; // FRotator: Pitch, Yaw, Roll
        [x, y, z].iter().all(|v| v.is_finite()).then_some([x, y, z, yaw])
    }

    /// Moves the player by writing the root component's location. Calling the engine's teleport
    /// from our thread froze the game, so this only writes memory, like every other mod.
    fn teleport_to(&mut self, to: [f64; 3]) {
        self.fall_guard = Some(Instant::now() + Duration::from_secs(30));
        self.hold = Some((to, Instant::now() + Duration::from_secs(3)));
        log(&format!("teleport to {to:?}"));
        self.place(to);
    }

    /// Writes `to` as the player's location (relative and cached world transform) and stops them.
    fn place(&mut self, to: [f64; 3]) -> Option<()> {
        let player = self.t.player;
        let ro = self.off(player, "RootComponent").ok()?;
        let root = self.ue.p.ptr(player + ro)?;
        let rel = self.off(root, "RelativeLocation").ok()?;
        if self.world_loc_off.is_none() {
            // The root has no parent, so its world translation equals RelativeLocation: find that copy.
            // It's the one followed by the transform's scale (1, 1, 1); the bounds' origin is another copy.
            let here: Vec<f64> = (0..3).map(|i| self.ue.p.read::<f64>(root + rel + i * 8).unwrap_or(f64::NAN)).collect();
            let mut buf = vec![0u8; 0x400];
            if !self.ue.p.read_into(root, &mut buf) {
                return None;
            }
            let d = |o: usize| f64::from_le_bytes(buf[o..o + 8].try_into().unwrap());
            self.world_loc_off = (0x20..0x400 - 0x40).step_by(16)
                .find(|&o| o as u64 != rel && (0..3).all(|i| (d(o + i * 8) - here[i]).abs() < 0.01 && (d(o + 0x20 + i * 8) - 1.0).abs() < 1e-6))
                .map(|o| o as u64);
            log(&format!("world location offset: {:X?}", self.world_loc_off));
        }
        let wo = self.world_loc_off?;
        for (i, v) in to.iter().enumerate() {
            self.ue.p.write(root + rel + i as u64 * 8, *v);
            self.ue.p.write(root + wo + i as u64 * 8, *v);
        }
        let movement = self.t.movement;
        if let Ok(lo) = self.off(movement, "LastUpdateLocation") {
            for (i, v) in to.iter().enumerate() {
                self.ue.p.write(movement + lo + i as u64 * 8, *v);
            }
        }
        if let Ok(vo) = self.off(movement, "Velocity") {
            for i in 0..3 {
                self.ue.p.write(movement + vo + i * 8, 0.0f64);
            }
        }
        Some(())
    }

    /// Adds items to an inventory through the game's own `UFGInventoryComponent::AddStack`.
    fn add_stack(&mut self, inv: u64, item: u64, count: i32) -> bool {
        if self.ue.process_event == 0 {
            return false;
        }
        if self.add_stack_fn.is_none() {
            self.add_stack_fn = Some(self.ue.func("FGInventoryComponent", "AddStack"));
        }
        let Some(Some(f)) = &self.add_stack_fn else { return false };
        let Some(st) = f.params.iter().find(|p| p.0.eq_ignore_ascii_case("stack")).map(|p| p.1 as usize) else { return false };
        let mut params = vec![0u8; f.size as usize];
        params[st + 8..st + 16].copy_from_slice(&item.to_le_bytes());
        params[st + 0x28..st + 0x2C].copy_from_slice(&count.to_le_bytes());
        if let Some(o) = f.params.iter().find(|p| p.0 == "allowPartialAdd") {
            params[o.1 as usize] = 1;
        }
        match self.ue.call(self.ue.process_event, inv, f, &params) {
            Ok(_) => true,
            Err(e) => {
                log(&format!("AddStack failed: {e}"));
                false
            }
        }
    }

    /// Free production: keep every machine stocked with its recipe's ingredients.
    fn free_machines(&mut self) -> Result<(), String> {
        if self.fed_at.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            return Ok(());
        }
        self.fed_at = Some(Instant::now());
        if self.machines_at.map_or(true, |t| t.elapsed() > Duration::from_secs(30)) {
            const KINDS: [&str; 11] = ["Smelter", "Foundry", "Constructor", "Assembler", "Manufacturer", "Refinery", "Blender", "Packager", "HadronCollider", "QuantumEncoder", "Converter"];
            self.machines = self.ue.find_by_classes(|n| n.starts_with("Build_") && KINDS.iter().any(|k| n.contains(k)));
            self.machines_at = Some(Instant::now());
            log(&format!("free production: {} machines", self.machines.len()));
        }
        // Machines keep their own copy of the ingredient list, so instead of changing the recipe,
        // keep each input slot stocked with what the machine's cached recipe asks for.
        const STACK: u64 = 0x30;
        let mut calls = 0;
        for m in self.machines.clone() {
            let (Ok(ro), Ok(io)) = (self.off(m, "mCachedRecipe"), self.off(m, "mInputInventory")) else { continue };
            let (Some(recipe), Some(inv)) = (self.ue.p.ptr(m + ro), self.ue.p.ptr(m + io)) else { continue };
            let arr = inv + self.off(inv, "mInventoryStacks")?;
            let (data, n) = (self.ue.p.ptr(arr).unwrap_or(0), self.ue.p.read::<i32>(arr + 8).unwrap_or(0).clamp(0, 16) as u64);
            for (i, (item, amount)) in self.recipe_items(recipe).into_iter().enumerate() {
                if i as u64 >= n {
                    break;
                }
                let slot = data + i as u64 * STACK;
                let want = self.stack_size(item).max(amount);
                let have = self.ue.p.read::<i32>(slot + 0x28).unwrap_or(0);
                if have >= want {
                    // Full but maybe never woken (filled by a plain write earlier): take one item
                    // out by memory and add it back through the game, once per machine.
                    if i == 0 && calls < 40 && !self.woken.contains(&m) && have > 0 {
                        self.ue.p.write(slot + 0x28, have - 1);
                        if self.add_stack(inv, item, 1) {
                            calls += 1;
                            self.woken.insert(m);
                        } else {
                            self.ue.p.write(slot + 0x28, have);
                        }
                    }
                    continue;
                }
                self.woken.insert(m);
                // Add through the game's own AddStack so the machine notices and starts.
                // Plain memory write as a fallback, capped per cycle to go easy on the game.
                if calls < 40 && self.add_stack(inv, item, want - have) {
                    calls += 1;
                    if calls == 1 { log(&format!("AddStack ok: +{} into 0x{inv:X} (had {have})", want - have)); }
                } else {
                    self.ue.p.write(slot + 8, item);
                    self.ue.p.write(slot + 0x28, want);
                }
            }
        }        Ok(())
    }
    /// Unlimited jetpack fuel: fuel never drops below the highest level seen.
    fn hold_jetpack_fuel(&mut self) -> Result<(), String> {
        if self.jetpacks_at.map_or(true, |t| t.elapsed() > Duration::from_secs(10)) {
            self.jetpacks = self.ue.find_by_class("Equip_JetPack_C");
            self.jetpacks_at = Some(Instant::now());
        }
        for jp in self.jetpacks.clone() {
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

    /// The player we hold is still the one you control. After a respawn or save load the old
    /// character lingers without a controller, so its name alone isn't enough.
    fn still_valid(&mut self) -> bool {
        let p = self.t.player;
        p != 0
            && self.ue.name_of(p).is_some_and(|n| n.starts_with("Char_Player_C"))
            && self.off(p, "Controller").is_ok_and(|o| self.ue.p.ptr(p + o).is_some())
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
            // We had a player and lost it: respawn or new save. Everything cached belongs to the
            // old world, so start a fresh session.
            if self.t.player != 0 {
                return Err("Game reloaded, reconnecting".into());
            }
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
        self.pos = self.locate();
        if let Some(to) = self.teleport.take() {
            self.teleport_to(to);
        }
        // Hotkey actions: save where you stand, and jump back there (kept on disk across restarts).
        for a in actions {
            match a.as_str() {
                "save-position" => if let Some([x, y, z, _]) = self.pos {
                    spot_file_save([x, y, z + 50.0]);
                    log(&format!("saved position {x:.0}, {y:.0}, {z:.0}"));
                },
                "teleport-to-saved-position" => if let Some(to) = spot_file_load() {
                    self.teleport_to(to);
                },
                _ => {}
            }
        }
        let holding = match self.hold {
            Some((to, until)) if Instant::now() < until => { self.place(to); true }
            _ => { self.hold = None; false }
        };

        // Remember the game's own movement values once per movement component.
        const MOVE: [&str; 4] = ["MaxWalkSpeed", "mMaxSprintSpeed", "JumpZVelocity", "GravityScale"];
        if self.base_for != movement {
            // Read the defaults from the class default object, not the live component:
            // the live one may still carry values from an earlier session.
            // The player blueprint's template component (same name, owned by Default__Char_Player_C)
            // has the character's real defaults; the component class default is only a fallback.
            let comp = self.ue.name_of(movement);
            let cdo_name = self.ue.class_of(movement).and_then(|c| self.ue.name_of(c)).map(|c| format!("Default__{c}"));
            let all = self.ue.all_objects();
            let template = all.iter().copied().find(|&o| {
                o != movement && self.ue.name_of(o) == comp
                    && self.ue.outer_of(o).and_then(|x| self.ue.name_of(x)).is_some_and(|n| n.starts_with("Default__Char_Player"))
            });
            let cdo = template.or_else(|| cdo_name.and_then(|n| all.iter().copied().find(|&o| self.ue.name_of(o).as_deref() == Some(n.as_str()))));
            self.base.clear();
            for name in MOVE {
                let o = self.off(movement, name)?;
                let v = cdo.and_then(|c| self.ue.p.read::<f32>(c + o)).filter(|v| *v > 0.0 || name == "GravityScale");
                self.base.insert(name, v.unwrap_or_else(|| self.ue.p.read::<f32>(movement + o).unwrap_or(0.0)));
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
            ("GravityScale", if holding { 0.0 } else { get("gravity-strength", 100.0) / 100.0 }),
        ];
        for (name, factor) in scaled {
            let o = self.off(movement, name)?;
            let want = (self.base[name] as f64 * factor) as f32;
            if self.ue.p.read::<f32>(movement + o) != Some(want) {
                let ok = self.ue.p.write(movement + o, want);
                log(&format!("write {name} = {want} at 0x{:X} ok={ok}", movement + o));
            }
        }
        if world != 0 {
            let o = self.off(world, "TimeDilation")?;
            self.ue.p.write(world + o, get("game-speed", 1.0).clamp(0.1, 5.0) as f32);
        }
        let god = get("infinite-health", 0.0) != 0.0;
        // True god mode: the engine skips all damage for actors with bCanBeDamaged off.
        let player = self.t.player;
        if let Some(p) = self.ue.class_of(player).and_then(|c| self.ue.prop(c, "bCanBeDamaged")) {
            let addr = player + p.offset as u64 + p.bool_byte as u64;
            if let Some(byte) = self.ue.p.read::<u8>(addr) {
                let want = if god { byte & !p.bool_mask } else { byte | p.bool_mask };
                if want != byte {
                    let ok = self.ue.p.write(addr, want);
                    log(&format!("bCanBeDamaged -> {} ok={ok}", !god));
                }
            }
        }
        // Satisfactory deals damage through its own health component, so the switch above isn't
        // enough: give the player an enormous max health and keep it full. Restored when turned off.
        const GOD_HEALTH: f32 = 10_000_000.0;
        let (max_o, cur_o) = (self.off(health, "mMaxHealth")?, self.off(health, "mCurrentHealth")?);
        let max = self.ue.p.read::<f32>(health + max_o).unwrap_or(0.0);
        if god {
            if max < GOD_HEALTH {
                self.normal_health.entry(health).or_insert(max);
                self.ue.p.write(health + max_o, GOD_HEALTH);
                log(&format!("god health on (normal max {max})"));
            }
            if self.ue.p.read::<f32>(health + cur_o).is_some_and(|c| c < GOD_HEALTH) {
                self.ue.p.write(health + cur_o, GOD_HEALTH);
            }
        } else if let Some(normal) = self.normal_health.remove(&health) {
            self.ue.p.write(health + max_o, normal);
            self.ue.p.write(health + cur_o, normal);
            log(&format!("god health off (max back to {normal})"));
        } else if max >= GOD_HEALTH {
            // Left over from before a HotSwap restart: back to the player's normal 100.
            self.ue.p.write(health + max_o, 100.0f32);
            self.ue.p.write(health + cur_o, 100.0f32);
            log("god health off (leftover reset to 100)");
        }

        // No fall damage: the character has no fall damage curve while on; the original comes back when off.
        let (fc, fo) = (self.off(player, "mFallDamageCurve")?, self.off(player, "mFallDamageCurveOverride")?);
        let guarded = self.fall_guard.is_some_and(|t| Instant::now() < t);
        if get("no-fall-damage", 0.0) != 0.0 || guarded {
            if self.fall_curves.map_or(true, |f| f.0 != player) {
                let orig = (player, self.ue.p.read::<u64>(player + fc).unwrap_or(0), self.ue.p.read::<u64>(player + fo).unwrap_or(0));
                if orig.1 != 0 || orig.2 != 0 {
                    self.fall_curves = Some(orig);
                }
            }
            for o in [fc, fo] {
                if self.ue.p.read::<u64>(player + o) != Some(0) {
                    self.ue.p.write(player + o, 0u64);
                }
            }
        } else if let Some((p, curve, over)) = self.fall_curves.take() {
            if p == player {
                self.ue.p.write(player + fc, curve);
                self.ue.p.write(player + fo, over);
            }
        }
        // The game's own cheat switches. When a mod is off, put back what the save had.
        if let Some(gs) = self.game_state() {
            for (id, name) in [
                ("easy-crafting", "mCheatNoCost"),
                ("no-power-required", "mCheatNoPower"),
                ("no-fuel-needed", "mCheatNoFuel"),
                ("turbo-production", "mCheatTurboProductionMode"),
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
        if get("easy-crafting", 0.0) != 0.0 {
            self.free_workbench()?;
        } else {
            self.restore_recipes(0);
        }
        if get("free-production", 0.0) != 0.0 {
            self.free_machines()?;
        }
        self.turbo_machines(get("turbo-production", 0.0) != 0.0)?;
        if get("unlimited-jetpack-fuel", 0.0) != 0.0 {
            self.hold_jetpack_fuel()?;
        }
        let _ = sink;
        Ok(true)
    }
}
