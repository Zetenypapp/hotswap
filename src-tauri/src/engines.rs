//! Mods described as data (see `spec` in ui/data.js), run on any engine HotSwap can read by name:
//! Unity on Mono or IL2CPP, and Unreal Engine 4.25+. A mod says *what* to change, e.g.
//! `GameManager.instance.money` kept at least 999999, and the engine's reader finds *where* it is,
//! using the engine's own class and field names. Nothing depends on a game version's addresses.
//!
//! Paths: `[Namespace/]Class.field.field...`. The first field is a static field of the class if it
//! is one, otherwise the field on every live object of that class. Each further field is read
//! through the object the previous one points to; `field[]` goes through every element of an
//! array or list. A last part that isn't a field is tried as a property (Unity, e.g.
//! `UnityEngine/Time.timeScale`).

use crate::{il2cpp::Il2Cpp, mono::Mono, trainer::log, ue::objects::Ue};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// One change a mod makes. `op`: for toggles "set" (hold `v`, put back when off), "min"/"max"
/// (keep at least/at most `v`), "freeze" (hold the value it had), "best" (hold the highest it has
/// been, e.g. ammo in the current magazine), "fill" (hold the value of the
/// sibling field `from`); for sliders "scale" (the original times the slider percent) and "value"
/// (the slider times `mul`), both put back at the slider's default `def`; for actions "add" and "put".
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct Spec {
    pub at: String,
    pub op: String,
    #[serde(default)]
    pub v: f64,
    #[serde(default)]
    pub def: f64,
    #[serde(default = "one")]
    pub mul: f64,
    /// Value type, when the engine can't tell (e.g. "f32", "i32", "bool").
    #[serde(default)]
    pub ty: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    /// Part of an action (applied once when the button is pressed).
    #[serde(default)]
    pub act: bool,
}

fn one() -> f64 {
    1.0
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ty {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    /// A reference to an object (or an array/list, for `[]`).
    Ptr,
    /// Unreal's packed bools: one bit of the byte.
    Bit(u8),
    /// An Unreal TArray stored in place.
    Array,
}

impl Ty {
    fn parse(s: &str) -> Option<Ty> {
        Some(match s {
            "bool" => Ty::Bool,
            "i8" => Ty::I8,
            "u8" => Ty::U8,
            "i16" => Ty::I16,
            "u16" => Ty::U16,
            "i32" => Ty::I32,
            "u32" => Ty::U32,
            "i64" => Ty::I64,
            "u64" => Ty::U64,
            "f32" => Ty::F32,
            "f64" => Ty::F64,
            _ => return None,
        })
    }

    /// From a Mono/IL2CPP type code (MonoTypeEnum).
    fn managed(code: u8) -> Ty {
        match code {
            0x02 => Ty::Bool,
            0x04 => Ty::I8,
            0x05 => Ty::U8,
            0x06 => Ty::I16,
            0x03 | 0x07 => Ty::U16,
            0x08 => Ty::I32,
            0x09 => Ty::U32,
            0x0A => Ty::I64,
            0x0B => Ty::U64,
            0x0C => Ty::F32,
            0x0D => Ty::F64,
            // Enums are value types; nearly all are ints.
            0x11 => Ty::I32,
            _ => Ty::Ptr,
        }
    }

    /// From an Unreal property class name.
    fn unreal(kind: &str, mask: u8) -> Ty {
        match kind {
            "BoolProperty" if mask != 0xFF && mask != 0 => Ty::Bit(mask),
            "BoolProperty" => Ty::Bool,
            "ByteProperty" | "EnumProperty" => Ty::U8,
            "Int8Property" => Ty::I8,
            "Int16Property" => Ty::I16,
            "UInt16Property" => Ty::U16,
            "IntProperty" => Ty::I32,
            "UInt32Property" => Ty::U32,
            "Int64Property" => Ty::I64,
            "UInt64Property" => Ty::U64,
            "FloatProperty" => Ty::F32,
            "DoubleProperty" => Ty::F64,
            "ArrayProperty" => Ty::Array,
            _ => Ty::Ptr,
        }
    }
}

pub struct Field {
    pub off: u64,
    pub ty: Ty,
    pub is_static: bool,
}

/// What an engine reader offers the generic mods.
pub trait Reader {
    fn p(&self) -> &crate::ue::mem::Proc;
    fn class(&mut self, ns: &str, name: &str) -> Option<u64>;
    fn class_of(&mut self, obj: u64) -> Option<u64>;
    fn field(&mut self, class: u64, name: &str) -> Option<Field>;
    fn static_base(&mut self, _class: u64) -> Option<u64> {
        None
    }
    /// Live objects of each class (and its subclasses, where the engine makes that cheap).
    fn instances(&mut self, classes: &[u64]) -> Vec<Vec<u64>>;
    /// Element slots of the array or list a slot refers to.
    fn elements(&mut self, slot: u64, ty: Ty) -> Vec<u64>;
    /// Reads or writes a property (Unity: get_X/set_X). `obj` 0 for static ones.
    fn prop(&mut self, _class: u64, _obj: u64, _name: &str, _set: Option<f32>) -> Option<f32> {
        None
    }
    /// Dev aid: what a class looks like, or the game's classes.
    fn dump(&mut self, class: Option<&str>) -> String;
}

/// Mono and IL2CPP share almost everything: only the runtime calls differ.
macro_rules! managed_reader {
    ($t:ty, $header:expr) => {
        impl Reader for $t {
            fn p(&self) -> &crate::ue::mem::Proc {
                &self.p
            }
            fn class(&mut self, ns: &str, name: &str) -> Option<u64> {
                <$t>::class(self, ns, name)
            }
            fn class_of(&mut self, obj: u64) -> Option<u64> {
                <$t>::class_of(self, obj)
            }
            fn field(&mut self, class: u64, name: &str) -> Option<Field> {
                let (off, code, is_static) = self.field_info(class, name)?;
                Some(Field { off: off as u64, ty: Ty::managed(code), is_static })
            }
            fn static_base(&mut self, class: u64) -> Option<u64> {
                self.static_data(class)
            }
            fn instances(&mut self, classes: &[u64]) -> Vec<Vec<u64>> {
                let header: fn(&$t, u64) -> u64 = $header;
                let words: Vec<u64> = classes.iter().map(|&c| header(self, c)).collect();
                let found = crate::remote::find_words(&self.p, &words, 4096);
                // Unity objects: m_CachedPtr (the native object) is set while the object is alive.
                // Checking it skips destroyed objects and stray copies of the class pointer.
                classes.iter().zip(found).map(|(&c, objs)| {
                    let native = self.field_info(c, "m_CachedPtr").map(|f| f.0 as u64);
                    objs.into_iter()
                        .filter(|&o| native.is_none_or(|n| self.p.ptr(o + n).is_some_and(|p| p > 0x10000 && p % 8 == 0 && self.p.read::<u64>(p).is_some())))
                        .take(256)
                        .collect()
                }).collect()
            }
            fn elements(&mut self, slot: u64, _ty: Ty) -> Vec<u64> {
                let Some(obj) = self.p.ptr(slot) else { return vec![] };
                // List<T>: its _items array and _size; arrays: length at +0x18, items from +0x20.
                let list = self.class_of(obj).and_then(|c| Some((self.field_info(c, "_items")?.0, self.field_info(c, "_size")?.0)));
                let (arr, n) = match list {
                    Some((items, size)) => (self.p.ptr(obj + items as u64), self.p.read::<i32>(obj + size as u64).unwrap_or(0) as u64),
                    None => (Some(obj), self.p.read::<u64>(obj + 0x18).unwrap_or(0)),
                };
                let Some(arr) = arr else { return vec![] };
                (0..n.min(4096)).map(|i| arr + 0x20 + i * 8).collect()
            }
            fn prop(&mut self, class: u64, obj: u64, name: &str, set: Option<f32>) -> Option<f32> {
                match set {
                    Some(v) => self.invoke(class, obj, &format!("set_{name}"), Some(v)).map(|_| v),
                    None => self.invoke(class, obj, &format!("get_{name}"), None).map(|r| f32::from_bits(r as u32)),
                }
            }
            fn dump(&mut self, class: Option<&str>) -> String {
                let Some(name) = class else { return self.game_classes().join("\n") };
                let (ns, n) = name.rsplit_once('/').unwrap_or(("", name));
                let Some(c) = <$t>::class(self, ns, n) else { return format!("no class {name}") };
                let mut out = format!("{}\n", self.class_name(c));
                for (f, off, ty) in self.fields_of(c) {
                    out += &format!("  0x{off:X} {f}: {ty}\n");
                }
                out
            }
        }
    };
}

managed_reader!(Mono, |m, c| m.vtable(c).unwrap_or(0));
managed_reader!(Il2Cpp, |_, c| c);

/// Unreal: objects come from the engine's object table, fields from its property reflection.
pub struct UeReader {
    pub ue: Ue,
    classes: HashMap<String, Option<u64>>,
    props: HashMap<(u64, String), Option<(u64, Ty)>>,
    /// Class pointer -> the wanted classes it is (or derives from).
    kinds: HashMap<u64, Vec<u64>>,
}

impl UeReader {
    pub fn new(ue: Ue) -> UeReader {
        UeReader { ue, classes: HashMap::new(), props: HashMap::new(), kinds: HashMap::new() }
    }
}

impl Reader for UeReader {
    fn p(&self) -> &crate::ue::mem::Proc {
        &self.ue.p
    }
    fn class(&mut self, _ns: &str, name: &str) -> Option<u64> {
        if let Some(&c) = self.classes.get(name) {
            return c;
        }
        let c = self.ue.find_class(name);
        self.classes.insert(name.to_string(), c);
        c
    }
    fn class_of(&mut self, obj: u64) -> Option<u64> {
        self.ue.class_of(obj)
    }
    fn field(&mut self, class: u64, name: &str) -> Option<Field> {
        let key = (class, name.to_string());
        if !self.props.contains_key(&key) {
            let p = self.ue.prop(class, name).map(|p| {
                let ty = Ty::unreal(&p.kind, p.bool_mask);
                (p.offset as u64 + if matches!(ty, Ty::Bit(_)) { p.bool_byte as u64 } else { 0 }, ty)
            });
            self.props.insert(key.clone(), p);
        }
        self.props[&key].map(|(off, ty)| Field { off, ty, is_static: false })
    }
    fn instances(&mut self, classes: &[u64]) -> Vec<Vec<u64>> {
        let mut out = vec![Vec::new(); classes.len()];
        for obj in self.ue.all_objects() {
            let Some(cls) = self.ue.class_of(obj) else { continue };
            if !self.kinds.contains_key(&cls) {
                // Walk the class's parents once and remember which wanted classes it is.
                let mut is = Vec::new();
                let mut c = Some(cls);
                for _ in 0..32 {
                    let Some(k) = c else { break };
                    if classes.contains(&k) {
                        is.push(k);
                    }
                    c = self.ue.super_of(k);
                }
                self.kinds.insert(cls, is);
            }
            let is = &self.kinds[&cls];
            if is.is_empty() || self.ue.name_of(obj).is_some_and(|n| n.starts_with("Default__")) {
                continue;
            }
            for (i, c) in classes.iter().enumerate() {
                if is.contains(c) && out[i].len() < 256 {
                    out[i].push(obj);
                }
            }
        }
        // Classes asked about later need their own walk.
        self.kinds.retain(|_, is| !is.is_empty());
        out
    }
    fn elements(&mut self, slot: u64, ty: Ty) -> Vec<u64> {
        // TArray in place: data pointer, then count. Anything else: an object reference.
        if ty != Ty::Array {
            return vec![];
        }
        let (Some(data), Some(n)) = (self.ue.p.ptr(slot), self.ue.p.read::<i32>(slot + 8)) else { return vec![] };
        (0..(n.max(0) as u64).min(4096)).map(|i| data + i * 8).collect()
    }
    fn dump(&mut self, class: Option<&str>) -> String {
        let Some(name) = class else { return "give a class name".into() };
        let Some(c) = self.class(name, name) else { return format!("no class {name}") };
        self.ue.props(c).iter().map(|p| format!("  0x{:X} {}.{}: {}\n", p.offset, p.owner, p.name, p.kind)).collect()
    }
}

/// Attaches the reader for an engine name from the library scan.
pub fn attach(engine: &str, pid: u32) -> Result<Box<dyn Reader + Send>, String> {
    match engine {
        "unity-mono" => Mono::attach(pid).map(|m| Box::new(m) as Box<dyn Reader + Send>),
        "unity-il2cpp" => Il2Cpp::attach(pid).map(|m| Box::new(m) as Box<dyn Reader + Send>),
        "unreal" => {
            let p = crate::ue::mem::Proc::open(pid)?;
            let (_, exe) = p.main_module().ok_or("couldn't read the game's modules")?;
            let name = exe.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            drop(p);
            Ue::attach_scan(pid, &name).map(|u| Box::new(UeReader::new(u)) as Box<dyn Reader + Send>)
        }
        other => Err(format!("no live reader for {other} games yet")),
    }
}

/// A connection to a running game for the generic mods: through memory, or through the page of a
/// web-tech game.
pub enum Live {
    Mem(Session),
    Web(crate::cdp::Session),
    Script(crate::bridge::Session),
}

impl Live {
    pub fn attach(engine: &str, pid: Option<u32>, install: Option<std::path::PathBuf>) -> Result<Live, String> {
        if crate::cdp::is_web(engine) {
            return crate::cdp::Session::attach().map(Live::Web);
        }
        if crate::bridge::is_script(engine) {
            return crate::bridge::Session::attach(engine, install, pid).map(Live::Script);
        }
        attach(engine, pid.ok_or("Waiting for the game to start")?).map(|r| Live::Mem(Session::new(r)))
    }

    pub fn tick(&mut self, specs: &HashMap<String, Vec<Spec>>, values: Option<&HashMap<String, f64>>, actions: &[String], guard: &[Spec]) -> Result<bool, String> {
        match self {
            Live::Mem(s) => s.tick(specs, values, actions, guard),
            Live::Web(s) => s.tick(specs, values, actions),
            Live::Script(s) => s.tick(specs, values, actions),
        }
    }

    /// Why mods are off right now, if a guard closed them.
    pub fn note(&self) -> Option<String> {
        match self {
            Live::Mem(s) => s.note.clone(),
            _ => None,
        }
    }

    pub fn restore(&mut self, specs: &HashMap<String, Vec<Spec>>) {
        match self {
            Live::Mem(s) => s.restore(specs),
            Live::Web(s) => s.restore(specs),
            Live::Script(s) => s.restore(specs),
        }
    }
}

/// Where a spec's value lives right now.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Target {
    Mem(u64, Ty),
    /// A property: class, object (0 for static), index into `Session::prop_names`.
    Prop(u64, u64, usize),
}

struct Resolved {
    targets: Vec<Target>,
    /// Objects the path started from and their class (checked to still be alive).
    roots: Vec<(u64, u64)>,
    at: Instant,
}

pub struct Session {
    pub r: Box<dyn Reader + Send>,
    resolved: HashMap<String, Resolved>,
    /// Values before we changed them, to put back.
    orig: HashMap<String, f64>,
    /// Live objects by class (with their own class), and when they were searched.
    found: HashMap<u64, (Vec<(u64, u64)>, Instant)>,
    fields: HashMap<(u64, String), Option<(u64, Ty, bool)>>,
    prop_names: Vec<String>,
    /// Paths that didn't resolve, logged once.
    missing: std::collections::HashSet<String>,
    /// Where the `guard` values live (found once, then only read).
    guards: HashMap<String, Vec<Target>>,
    /// Why mods are off right now (the guard failed), for the status line.
    pub note: Option<String>,
}

impl Session {
    pub fn new(r: Box<dyn Reader + Send>) -> Session {
        Session { r, resolved: HashMap::new(), orig: HashMap::new(), found: HashMap::new(), fields: HashMap::new(), prop_names: Vec::new(), missing: Default::default(), guards: HashMap::new(), note: None }
    }

    /// Live objects of a class, as (object, its own class). Searching is slow, so the result is
    /// kept while its objects are alive: for 30 seconds, or 5 when nothing was found.
    fn objects(&mut self, class: u64) -> Vec<(u64, u64)> {
        if let Some((objs, at)) = self.found.get(&class).cloned() {
            let fresh = at.elapsed() < Duration::from_secs(if objs.is_empty() { 5 } else { 30 });
            if fresh && objs.iter().all(|&(o, c)| self.r.class_of(o) == Some(c)) {
                return objs;
            }
        }
        let objs: Vec<(u64, u64)> = self.r.instances(&[class]).pop().unwrap_or_default().into_iter().filter_map(|o| Some((o, self.r.class_of(o)?))).collect();
        self.found.insert(class, (objs.clone(), Instant::now()));
        objs
    }

    fn field(&mut self, class: u64, name: &str) -> Option<Field> {
        let key = (class, name.to_string());
        if !self.fields.contains_key(&key) {
            let f = self.r.field(class, name).map(|f| (f.off, f.ty, f.is_static));
            self.fields.insert(key.clone(), f);
        }
        self.fields[&key].map(|(off, ty, is_static)| Field { off, ty, is_static })
    }

    fn prop(&mut self, class: u64, obj: u64, name: &str) -> Target {
        let i = self.prop_names.iter().position(|n| n == name).unwrap_or_else(|| {
            self.prop_names.push(name.to_string());
            self.prop_names.len() - 1
        });
        Target::Prop(class, obj, i)
    }

    /// Finds every place a path points to.
    fn resolve(&mut self, path: &str, ty: Option<Ty>) -> Option<Resolved> {
        let (ns, rest) = path.rsplit_once('/').unwrap_or(("", path));
        let mut parts = rest.split('.');
        let class = self.r.class(ns, parts.next()?)?;
        let parts: Vec<&str> = parts.collect();
        let mut roots = Vec::new();
        // Slots: addresses of values, with their type.
        let mut slots: Vec<(u64, Ty)> = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            let (name, each) = part.strip_suffix("[]").map_or((*part, false), |n| (n, true));
            let last = i + 1 == parts.len();
            if i == 0 {
                match self.field(class, name) {
                    Some(f) if f.is_static => slots.push((self.r.static_base(class)? + f.off, f.ty)),
                    Some(f) => {
                        for (o, c) in self.objects(class) {
                            roots.push((o, c));
                            slots.push((o + f.off, f.ty));
                        }
                    }
                    // A static property, e.g. Time.timeScale.
                    None if last => return Some(Resolved { targets: vec![self.prop(class, 0, name)], roots, at: Instant::now() }),
                    None => return None,
                }
            } else {
                let mut next = Vec::new();
                for (slot, _) in std::mem::take(&mut slots) {
                    let Some(obj) = self.r.p().ptr(slot) else { continue };
                    let Some(c) = self.r.class_of(obj) else { continue };
                    // No such field on this object (e.g. a melee weapon has no ammo): skip it.
                    if let Some(f) = self.field(c, name) {
                        next.push((obj + f.off, f.ty));
                    }
                }
                slots = next;
            }
            if each {
                slots = std::mem::take(&mut slots).into_iter().flat_map(|(s, t)| self.r.elements(s, t)).map(|s| (s, Ty::Ptr)).collect();
            }
        }
        let targets = slots.into_iter().map(|(a, t)| Target::Mem(a, ty.unwrap_or(t))).collect();
        Some(Resolved { targets, roots, at: Instant::now() })
    }

    fn targets(&mut self, spec: &Spec, path: &str) -> Vec<Target> {
        let key = format!("{path}|{:?}", spec.ty);
        let stale = match self.resolved.get(&key) {
            None => true,
            Some(r) => {
                let roots = r.roots.clone();
                let dead = roots.iter().any(|&(o, c)| self.r.class_of(o) != Some(c));
                let old = r.at.elapsed() > if r.targets.is_empty() { Duration::from_secs(5) } else { Duration::from_secs(30) };
                // Paths from static fields are cheap to follow again (everything on the way is
                // cached), and following them each time picks up a new player object right away.
                dead || old || roots.is_empty()
            }
        };
        if stale {
            let r = self.resolve(path, spec.ty.as_deref().and_then(Ty::parse)).unwrap_or(Resolved { targets: vec![], roots: vec![], at: Instant::now() });
            if r.targets.is_empty() && self.missing.insert(path.to_string()) {
                log(&format!("engine mod: nothing at {path} yet"));
            }
            self.resolved.insert(key.clone(), r);
        }
        self.resolved[&key].targets.clone()
    }

    fn read(&mut self, t: Target) -> Option<f64> {
        let p = self.r.p();
        Some(match t {
            Target::Mem(a, ty) => match ty {
                Ty::Bool | Ty::U8 => p.read::<u8>(a)? as f64,
                Ty::I8 => p.read::<i8>(a)? as f64,
                Ty::I16 => p.read::<i16>(a)? as f64,
                Ty::U16 => p.read::<u16>(a)? as f64,
                Ty::I32 => p.read::<i32>(a)? as f64,
                Ty::U32 => p.read::<u32>(a)? as f64,
                Ty::I64 => p.read::<i64>(a)? as f64,
                Ty::U64 => p.read::<u64>(a)? as f64,
                Ty::F32 => p.read::<f32>(a)? as f64,
                Ty::F64 => p.read::<f64>(a)?,
                Ty::Bit(m) => (p.read::<u8>(a)? & m != 0) as u8 as f64,
                Ty::Ptr | Ty::Array => return None,
            },
            Target::Prop(c, o, i) => {
                let name = self.prop_names[i].clone();
                self.r.prop(c, o, &name, None)? as f64
            }
        })
    }

    fn write(&mut self, t: Target, v: f64) {
        if self.read(t) == Some(v) {
            return;
        }
        let p = self.r.p();
        match t {
            Target::Mem(a, ty) => {
                let _ = match ty {
                    Ty::Bool => p.write(a, (v != 0.0) as u8),
                    Ty::U8 => p.write(a, v as u8),
                    Ty::I8 => p.write(a, v as i8),
                    Ty::I16 => p.write(a, v as i16),
                    Ty::U16 => p.write(a, v as u16),
                    Ty::I32 => p.write(a, v as i32),
                    Ty::U32 => p.write(a, v as u32),
                    Ty::I64 => p.write(a, v as i64),
                    Ty::U64 => p.write(a, v as u64),
                    Ty::F32 => p.write(a, v as f32),
                    Ty::F64 => p.write(a, v),
                    Ty::Bit(m) => {
                        let b = p.read::<u8>(a).unwrap_or(0);
                        p.write(a, if v != 0.0 { b | m } else { b & !m })
                    }
                    Ty::Ptr | Ty::Array => false,
                };
            }
            Target::Prop(c, o, i) => {
                let name = self.prop_names[i].clone();
                self.r.prop(c, o, &name, Some(v as f32));
            }
        }
    }

    /// Applies one mod's specs. `on`: toggle state or slider value; None when the mod is off/unused.
    fn apply(&mut self, id: &str, specs: &[Spec], value: f64) {
        for (k, s) in specs.iter().enumerate() {
            let targets = self.targets(s, &s.at.clone());
            let slider = matches!(s.op.as_str(), "scale" | "value");
            let on = if slider { value != s.def } else { value != 0.0 };
            let siblings = match (&s.op[..], &s.from) {
                ("fill", Some(from)) => {
                    let sib = format!("{}.{from}", s.at.rsplit_once('.').map_or("", |x| x.0));
                    self.targets(s, &sib)
                }
                _ => vec![],
            };
            for (i, &t) in targets.iter().enumerate() {
                let key = format!("{id}#{k}#{t:?}");
                if !on {
                    // Put back what we changed (min/max/fill/freeze leave the game's value alone).
                    if let Some(v) = self.orig.remove(&key) {
                        if matches!(s.op.as_str(), "set" | "scale" | "value") {
                            self.write(t, v);
                        }
                    }
                    continue;
                }
                let Some(cur) = self.read(t) else { continue };
                let orig = *self.orig.entry(key).or_insert(cur);
                let want = match s.op.as_str() {
                    "set" => s.v,
                    "min" => cur.max(s.v),
                    "max" => cur.min(s.v),
                    "freeze" => orig,
                    "best" => {
                        let best = orig.max(cur);
                        self.orig.insert(format!("{id}#{k}#{t:?}"), best);
                        best
                    }
                    "fill" => match siblings.get(i).copied().and_then(|t| self.read(t)) {
                        Some(v) => v,
                        None => continue,
                    },
                    "scale" => orig * value / 100.0,
                    "value" => value * s.mul,
                    _ => continue,
                };
                self.write(t, want);
            }
        }
    }

    /// True if every guard value is what it must be (`op` "is", value `v`), e.g. "the game is in
    /// singleplayer". Only the first check looks anything up; after that it is plain reads.
    fn allowed(&mut self, guard: &[Spec]) -> bool {
        guard.iter().all(|g| {
            if !self.guards.contains_key(&g.at) {
                let found = self.resolve(&g.at, g.ty.as_deref().and_then(Ty::parse)).map(|r| r.targets).unwrap_or_default();
                if found.is_empty() {
                    return false;
                }
                self.guards.insert(g.at.clone(), found);
            }
            let ts = self.guards[&g.at].clone();
            ts.iter().all(|&t| matches!(t, Target::Mem(..)) && self.read(t) == Some(g.v))
        })
    }

    pub fn tick(&mut self, specs: &HashMap<String, Vec<Spec>>, values: Option<&HashMap<String, f64>>, actions: &[String], guard: &[Spec]) -> Result<bool, String> {
        if self.r.p().read::<u8>(0x7FFE_0000).is_none() {
            return Err("the game closed".into());
        }
        if !guard.is_empty() && !self.allowed(guard) {
            // Not allowed now (e.g. online): touch nothing. What the mods changed belonged to the
            // world that was left, so it is forgotten rather than written back.
            if self.note.is_none() {
                log("engine mods: guard closed, mods off");
            }
            self.note = Some("Singleplayer only: mods are off until you play offline.".into());
            self.orig.clear();
            self.resolved.clear();
            self.found.clear();
            return Ok(false);
        }
        self.note = None;
        for (id, list) in specs {
            if list.iter().any(|s| s.act) {
                continue;
            }
            let v = values.and_then(|v| v.get(id)).copied();
            let off = list.first().map_or(0.0, |s| if matches!(s.op.as_str(), "scale" | "value") { s.def } else { 0.0 });
            self.apply(id, list, v.unwrap_or(off));
        }
        for id in actions {
            let Some(list) = specs.get(id) else { continue };
            for s in list.clone() {
                for t in self.targets(&s, &s.at) {
                    let Some(cur) = self.read(t) else { continue };
                    match s.op.as_str() {
                        "add" => self.write(t, cur + s.v),
                        "put" => self.write(t, s.v),
                        _ => {}
                    }
                }
            }
        }
        Ok(true)
    }

    /// Puts back everything still changed (the game is still running, HotSwap stops modding it).
    pub fn restore(&mut self, specs: &HashMap<String, Vec<Spec>>) {
        if self.note.is_some() {
            return;
        }
        for (id, list) in specs.iter().filter(|(_, l)| !l.iter().any(|s| s.act)) {
            let off = list.first().map_or(0.0, |s| if matches!(s.op.as_str(), "scale" | "value") { s.def } else { 0.0 });
            self.apply(id, list, off);
        }
    }
}

/// Which engine a game is made with, from the files in its install folder (and one level down,
/// where some games keep the real game). None for engines HotSwap doesn't know.
pub fn detect(install: &std::path::Path) -> Option<&'static str> {
    if install.as_os_str().len() <= 3 {
        return None;
    }
    let mut dirs = vec![install.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(install) {
        dirs.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).take(40));
    }
    // GTA V (Legacy and Enhanced): story mode through Script Hook V (bridge.rs).
    if install.join("GTA5.exe").exists() || install.join("GTA5_Enhanced.exe").exists() {
        return Some("gta");
    }
    for dir in &dirs {
        let has = |rel: &str| dir.join(rel).exists();
        let names: Vec<String> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect();
        let any = |f: &dyn Fn(&str) -> bool| names.iter().any(|n| f(n));
        if has("GameAssembly.dll") {
            return Some("unity-il2cpp");
        }
        if any(&|n| n.ends_with("_data")) && (has("MonoBleedingEdge") || has("mono") || names.iter().any(|n| n.ends_with("_data") && dir.join(n).join("Managed").join("UnityEngine.dll").exists())) {
            return Some("unity-mono");
        }
        if any(&|n| n == "engine") && names.iter().any(|n| dir.join(n).join("Binaries").join("Win64").is_dir() && n != "engine") {
            return Some("unreal");
        }
        if has("www/js/rpg_core.js") || has("js/rpg_core.js") {
            return Some("rpgmaker-mv");
        }
        if has("js/rmmz_core.js") || has("www/js/rmmz_core.js") {
            return Some("rpgmaker-mz");
        }
        if has("renpy") && has("game") {
            return Some("renpy");
        }
        if any(&|n| n.ends_with(".pck")) {
            return Some("godot");
        }
        if has("data.win") || any(&|n| n == "game.unx") {
            return Some("gamemaker");
        }
        if any(&|n| n.starts_with("re_chunk_000")) {
            return Some("re-engine");
        }
        if has("LICENSE.electron.txt") || has("resources/app.asar") {
            return Some("electron");
        }
        if has("nw.dll") && (has("package.json") || has("package.nw")) {
            return Some("nwjs");
        }
        if any(&|n| ["skyrimse.exe", "skyrim.exe", "fallout4.exe", "fallout3.exe", "falloutnv.exe", "starfield.exe", "oblivion.exe"].contains(&n)) {
            return Some("creation");
        }
        if any(&|n| n == "fna.dll" || n == "monogame.framework.dll" || n.starts_with("microsoft.xna.framework")) {
            return Some("xna");
        }
    }
    None
}
