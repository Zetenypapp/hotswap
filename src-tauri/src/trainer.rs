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
                        let started = Instant::now();
                        let result = session.as_mut().map(|s| s.tick(if paused { None } else { Some(&values) }, &actions));
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
                    _ => {
                        session = None;
                        Status { game: game.clone(), connected: false, message: String::new() }
                    }
                };
                if status != last {
                    log(&format!("status connected={} msg={}", status.connected, status.message));
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
    /// God mode: each health component's real max health, to put back afterwards.
    normal_health: HashMap<u64, f32>,
    gs_search: Option<Instant>,
    jetpacks: Vec<u64>,
    jetpacks_at: Option<Instant>,
    recipe_addr: HashMap<u64, u64>,
    /// Recipes made ingredient-free: count address -> (real count, [by workbench, by machines]).
    zeroed: HashMap<u64, (i32, [bool; 2])>,
    benches: Vec<u64>,
    benches_at: Option<Instant>,
    machines: Vec<u64>,
    machines_at: Option<Instant>,
    fed_at: Option<Instant>,
}

impl Session {
    fn new(ue: Ue) -> Session {
        Session { ue, t: Targets::default(), resolved_at: None, offsets: HashMap::new(), base: HashMap::new(), base_for: 0, gs: 0, orig_bools: HashMap::new(), held: HashMap::new(), held_for: 0, fuel_max: HashMap::new(), normal_health: HashMap::new(), gs_search: None, jetpacks: Vec::new(), jetpacks_at: None,
            recipe_addr: HashMap::new(), zeroed: HashMap::new(), benches: Vec::new(), benches_at: None, machines: Vec::new(), machines_at: None, fed_at: None }
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

    /// Free production: every machine's current recipe needs no ingredients.
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
        for m in self.machines.clone() {
            let Ok(ro) = self.off(m, "mCurrentRecipe") else { continue };
            if let Some(recipe) = self.ue.p.ptr(m + ro) {
                self.zero_recipe(recipe, 1);
            }
        }
        Ok(())
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
            ("GravityScale", get("gravity-strength", 100.0) / 100.0),
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
        let god = get("infinite-health", 0.0) != 0.0 || get("no-fall-damage", 0.0) != 0.0;
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
        if get("easy-crafting", 0.0) != 0.0 {
            self.free_workbench()?;
        } else {
            self.restore_recipes(0);
        }
        if get("free-production", 0.0) != 0.0 {
            self.free_machines()?;
        } else {
            self.restore_recipes(1);
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
