//! Walks a running UE5 game's object table and name pool. Layouts are UE 5.x defaults
//! for 64-bit shipping builds; `attach` sanity-checks them before anything is used.

use super::{mem::Proc, symbols};
use std::{collections::HashMap, path::Path};

const CHUNK: u64 = 64 * 1024;
const ITEM_SIZE: u64 = 0x18; // FUObjectItem: Object, Flags, ClusterRootIndex, SerialNumber
// UObjectBase
const OBJ_CLASS: u64 = 0x10;
const OBJ_NAME: u64 = 0x18;
const OBJ_OUTER: u64 = 0x20;
// UStruct (after UField::Next and FStructBaseChain)
const STRUCT_SUPER: u64 = 0x40;
const STRUCT_CHILD_PROPS: u64 = 0x50;
// FField / FProperty / FBoolProperty (UE 5.3+: FFieldVariant Owner is 8 bytes)
const FIELD_CLASS: u64 = 0x08;
const FIELD_NEXT: u64 = 0x18;
const FIELD_NAME: u64 = 0x20;
const PROP_OFFSET: u64 = 0x44;
const BOOL_BYTE_OFFSET: u64 = 0x71;
const BOOL_FIELD_MASK: u64 = 0x73;

/// A reflected property: where it lives inside an object and what type it is.
#[derive(Debug, Clone)]
pub struct Prop {
    pub name: String,
    /// FField class name, e.g. "FloatProperty", "BoolProperty", "ObjectProperty"
    pub kind: String,
    pub offset: i32,
    pub bool_byte: u8,
    pub bool_mask: u8,
    /// Class that declares it
    pub owner: String,
}

pub struct Ue {
    pub p: Proc,
    objects: u64,
    names: u64,
    name_cache: std::sync::Mutex<HashMap<u32, String>>,
}

impl Ue {
    /// Attaches to a UE5 game process whose engine modules have .pdb files next to them.
    pub fn attach(pid: u32) -> Result<Ue, String> {
        let p = Proc::open(pid)?;
        let (core_uobj, cu_path) = p.module("-CoreUObject-").ok_or("CoreUObject module not loaded (not an Unreal game?)")?;
        let (core, c_path) = p.module("-Core-").ok_or("Core module not loaded")?;
        let rva = |path: &Path, sym: &str| symbols::find_rva(&path.with_extension("pdb"), sym);
        let ue = Ue {
            objects: core_uobj + rva(&cu_path, "GUObjectArray")? as u64,
            names: core + rva(&c_path, "NamePoolData")? as u64,
            p,
            name_cache: Default::default(),
        };
        // FName 0 is always "None"; if not, the layout guesses are wrong for this game.
        match ue.fname(0).as_deref() {
            Some("None") => Ok(ue),
            other => Err(format!("Unexpected name pool layout (name 0 = {other:?})")),
        }
    }

    /// Text of an FName comparison index (UE5 FNamePool).
    pub fn fname(&self, index: u32) -> Option<String> {
        if let Some(s) = self.name_cache.lock().unwrap().get(&index) {
            return Some(s.clone());
        }
        let block = self.p.ptr(self.names + 0x10 + (index as u64 >> 16) * 8)?;
        let entry = block + (index as u64 & 0xFFFF) * 2;
        let header: u16 = self.p.read(entry)?;
        let (wide, len) = (header & 1 == 1, (header >> 6) as usize);
        if len == 0 || len > 1024 {
            return None;
        }
        let s = if wide {
            let mut buf = vec![0u8; len * 2];
            self.p.read_into(entry + 2, &mut buf).then_some(())?;
            String::from_utf16_lossy(&buf.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
        } else {
            let mut buf = vec![0u8; len];
            self.p.read_into(entry + 2, &mut buf).then_some(())?;
            String::from_utf8_lossy(&buf).into_owned()
        };
        self.name_cache.lock().unwrap().insert(index, s.clone());
        Some(s)
    }

    /// Name of an FName stored at `addr` (comparison index + number suffix).
    pub fn name_at(&self, addr: u64) -> Option<String> {
        let index: u32 = self.p.read(addr)?;
        let number: u32 = self.p.read(addr + 4)?;
        let base = self.fname(index)?;
        Some(if number > 0 { format!("{base}_{}", number - 1) } else { base })
    }

    pub fn count(&self) -> u32 {
        self.p.read::<i32>(self.objects + 0x10 + 0x14).unwrap_or(0).max(0) as u32
    }

    /// Address of the object at slot `i`, if that slot is in use.
    pub fn object(&self, i: u32) -> Option<u64> {
        let chunks = self.p.ptr(self.objects + 0x10)?;
        let chunk = self.p.ptr(chunks + (i as u64 / CHUNK) * 8)?;
        self.p.ptr(chunk + (i as u64 % CHUNK) * ITEM_SIZE)
    }

    pub fn name_of(&self, obj: u64) -> Option<String> {
        self.name_at(obj + OBJ_NAME)
    }

    pub fn class_of(&self, obj: u64) -> Option<u64> {
        self.p.ptr(obj + OBJ_CLASS)
    }

    pub fn outer_of(&self, obj: u64) -> Option<u64> {
        self.p.ptr(obj + OBJ_OUTER)
    }

    /// Every live object address, read a whole chunk (64k slots) at a time.
    pub fn all_objects(&self) -> Vec<u64> {
        let (count, Some(chunks)) = (self.count() as u64, self.p.ptr(self.objects + 0x10)) else { return vec![] };
        let mut out = Vec::with_capacity(count as usize);
        for c in 0..count.div_ceil(CHUNK) {
            let Some(chunk) = self.p.ptr(chunks + c * 8) else { continue };
            let n = CHUNK.min(count - c * CHUNK);
            let mut buf = vec![0u8; (n * ITEM_SIZE) as usize];
            if !self.p.read_into(chunk, &mut buf) {
                continue;
            }
            out.extend(buf.chunks(ITEM_SIZE as usize).map(|it| u64::from_le_bytes(it[..8].try_into().unwrap())).filter(|&o| o != 0));
        }
        out
    }

    /// Class object named `name` (a UClass or BlueprintGeneratedClass).
    pub fn find_class(&self, name: &str) -> Option<u64> {
        self.all_objects().into_iter().find(|&o| {
            self.name_of(o).as_deref() == Some(name)
                && self.class_of(o).and_then(|c| self.name_of(c)).is_some_and(|c| c.ends_with("Class") || c == "ScriptStruct")
        })
    }

    pub fn super_of(&self, cls: u64) -> Option<u64> {
        self.p.ptr(cls + STRUCT_SUPER)
    }

    /// Every property on `cls` and its parents, nearest class first.
    pub fn props(&self, cls: u64) -> Vec<Prop> {
        let mut out = Vec::new();
        let mut cur = Some(cls);
        // Hard limits so a wrong layout guess can't loop forever.
        let mut depth = 0;
        while let Some(c) = cur.filter(|_| depth < 32) {
            depth += 1;
            let mut f = self.p.ptr(c + STRUCT_CHILD_PROPS);
            let mut n = 0;
            while let Some(field) = f.filter(|_| n < 2000) {
                n += 1;
                let kind = self.p.ptr(field + FIELD_CLASS).and_then(|fc| self.name_at(fc)).unwrap_or_default();
                out.push(Prop {
                    name: self.name_at(field + FIELD_NAME).unwrap_or_default(),
                    kind,
                    offset: self.p.read::<i32>(field + PROP_OFFSET).unwrap_or(-1),
                    bool_byte: self.p.read::<u8>(field + BOOL_BYTE_OFFSET).unwrap_or(0),
                    bool_mask: self.p.read::<u8>(field + BOOL_FIELD_MASK).unwrap_or(0),
                    owner: self.name_of(c).unwrap_or_default(),
                });
                f = self.p.ptr(field + FIELD_NEXT);
            }
            cur = self.super_of(c);
        }
        out
    }

    pub fn prop(&self, cls: u64, name: &str) -> Option<Prop> {
        self.props(cls).into_iter().find(|p| p.name == name && p.offset >= 0)
    }

    /// All live objects whose class is exactly `class_name`, skipping class default objects.
    pub fn find_by_class(&self, class_name: &str) -> Vec<u64> {
        // Many objects share a class, so name each class pointer only once.
        let mut is_match: HashMap<u64, bool> = HashMap::new();
        let mut out = Vec::new();
        for obj in self.all_objects() {
            let Some(cls) = self.class_of(obj) else { continue };
            let hit = *is_match.entry(cls).or_insert_with(|| self.name_of(cls).as_deref() == Some(class_name));
            if hit && !self.name_of(obj).is_some_and(|n| n.starts_with("Default__")) {
                out.push(obj);
            }
        }
        out
    }
}
