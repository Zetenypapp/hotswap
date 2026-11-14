//! Reads Unity games that run on Mono. Instead of guessing Mono's internal layouts (they change
//! between Unity versions), HotSwap asks Mono itself: it calls the runtime's exported API
//! (mono_class_from_name, mono_class_get_field_from_name, ...) from a short-lived thread in the
//! game, attached to Mono for the call (see remote.rs). The answers (field offsets, static field
//! addresses, class vtables) are cached, and the mods themselves are plain memory reads and writes.

use crate::remote::{self, Attach, Buf};
pub use crate::remote::Arg;
use crate::ue::mem::Proc;
use std::collections::HashMap;

pub struct Mono {
    pub p: Proc,
    exports: HashMap<String, u64>,
    domain: u64,
    /// Assembly-CSharp, where a Unity game's own classes live.
    image: u64,
    /// Every other loaded assembly's image, searched when a class isn't in Assembly-CSharp.
    others: Option<Vec<u64>>,
    classes: HashMap<String, Option<u64>>,
    fields: HashMap<(u64, String), Option<u32>>,
    statics: HashMap<u64, u64>,
}

impl Mono {
    pub fn attach(pid: u32) -> Result<Mono, String> {
        let p = Proc::open(pid)?;
        let (base, _) = p.module("mono-2.0-bdwgc.dll").or_else(|| p.module("mono.dll")).ok_or("Mono isn't loaded (not a Mono Unity game?)")?;
        let exports = remote::exports(&p, base);
        if !exports.contains_key("mono_class_from_name") {
            return Err("Mono API not found".into());
        }
        let mut m = Mono { p, exports, domain: 0, image: 0, others: None, classes: HashMap::new(), fields: HashMap::new(), statics: HashMap::new() };
        m.domain = m.call("mono_get_root_domain", &[])?;
        if m.domain == 0 {
            return Err("Mono hasn't started yet".into());
        }
        m.image = m.call("mono_image_loaded", &[Arg::S("Assembly-CSharp")])?;
        if m.image == 0 {
            return Err("the game's code isn't loaded yet".into());
        }
        Ok(m)
    }

    /// Calls an exported Mono function with up to 4 arguments and returns what it returned.
    pub fn call(&self, func: &str, args: &[Arg]) -> Result<u64, String> {
        let f = *self.exports.get(func).ok_or_else(|| format!("{func} not exported"))?;
        // Mono only allows calls from threads it knows about.
        let attach = (self.domain != 0).then(|| Attach { attach: self.exports["mono_thread_attach"], detach: self.exports["mono_thread_detach"], domain: self.domain });
        remote::call(&self.p, f, args, attach)
    }

    /// Images of every loaded assembly, found with mono_assembly_foreach and a tiny callback that
    /// collects the assemblies into a buffer.
    fn other_images(&mut self) -> &[u64] {
        if self.others.is_none() {
            let mut out = Vec::new();
            // rcx = assembly, rdx = list: if list.count < 1024 { list.items[count++] = assembly }
            let cb = [0x48, 0x8B, 0x02, 0x48, 0x3D, 0x00, 0x04, 0x00, 0x00, 0x73, 0x0C, 0x48, 0x89, 0x4C, 0xC2, 0x08, 0x48, 0xFF, 0xC0, 0x48, 0x89, 0x02, 0xC3, 0xC3];
            if let (Some(code), Some(list)) = (Buf::code(&self.p, &cb), Buf::new(&self.p, 8 * 1025)) {
                if self.call("mono_assembly_foreach", &[Arg::N(code.at), Arg::N(list.at)]).is_ok() {
                    let n = self.p.read::<u64>(list.at).unwrap_or(0).min(1024);
                    for i in 0..n {
                        let Some(asm) = self.p.ptr(list.at + 8 + i * 8) else { continue };
                        if let Ok(img) = self.call("mono_assembly_get_image", &[Arg::N(asm)]) {
                            if img != 0 && img != self.image {
                                out.push(img);
                            }
                        }
                    }
                }
            }
            self.others = Some(out);
        }
        self.others.as_deref().unwrap_or_default()
    }

    /// A class from the game's code, e.g. ("", "Player"). Assembly-CSharp first, then every other
    /// loaded assembly (Unity's own classes, plugins).
    pub fn class(&mut self, ns: &str, name: &str) -> Option<u64> {
        let key = format!("{ns}.{name}");
        if let Some(&c) = self.classes.get(&key) {
            return c;
        }
        let find = |m: &Mono, img: u64| m.call("mono_class_from_name", &[Arg::N(img), Arg::S(ns), Arg::S(name)]).ok().filter(|&c| c != 0);
        let mut c = find(self, self.image);
        if c.is_none() {
            let imgs = self.other_images().to_vec();
            c = imgs.into_iter().find_map(|img| find(self, img));
        }
        self.classes.insert(key, c);
        c
    }

    /// Byte offset of an instance field within an object (includes the object header).
    pub fn field(&mut self, class: u64, name: &str) -> Option<u32> {
        if let Some(v) = self.fields.get(&(class, name.to_string())) {
            return *v;
        }
        let f = self.call("mono_class_get_field_from_name", &[Arg::N(class), Arg::S(name)]).ok().filter(|&f| f != 0);
        let off = f.and_then(|f| self.call("mono_field_get_offset", &[Arg::N(f)]).ok()).map(|o| o as u32);
        self.fields.insert((class, name.to_string()), off);
        off
    }

    /// A field's offset, type code (MonoTypeEnum) and whether it is static.
    pub fn field_info(&mut self, class: u64, name: &str) -> Option<(u32, u8, bool)> {
        let f = self.call("mono_class_get_field_from_name", &[Arg::N(class), Arg::S(name)]).ok().filter(|&f| f != 0)?;
        let off = self.call("mono_field_get_offset", &[Arg::N(f)]).ok()? as u32;
        let ty = self.call("mono_field_get_type", &[Arg::N(f)]).ok().filter(|&t| t != 0)?;
        let code = self.call("mono_type_get_type", &[Arg::N(ty)]).ok()? as u8;
        let flags = self.call("mono_field_get_flags", &[Arg::N(f)]).unwrap_or(0);
        Some((off, code, flags & 0x10 != 0))
    }

    /// The class of a live object (its vtable's first word).
    pub fn class_of(&self, obj: u64) -> Option<u64> {
        self.p.ptr(self.p.ptr(obj)?)
    }

    /// Calls a method with one 4-byte argument (a float's or an int's bits) or none, and returns what
    /// it returned: an object, or a boxed value (its data at +0x10).
    pub fn invoke_raw(&mut self, class: u64, obj: u64, method: &str, arg: Option<u32>) -> Option<u64> {
        let m = self.call("mono_class_get_method_from_name", &[Arg::N(class), Arg::S(method), Arg::N(arg.is_some() as u64)]).ok().filter(|&m| m != 0)?;
        let buf = Buf::new(&self.p, 32)?;
        let params = match arg {
            Some(v) => {
                self.p.write(buf.at + 16, v);
                self.p.write(buf.at, buf.at + 16);
                buf.at
            }
            None => 0,
        };
        // Passing a slot for the exception makes the runtime catch it instead of crashing the game.
        let r = self.call("mono_runtime_invoke", &[Arg::N(m), Arg::N(obj), Arg::N(params), Arg::N(buf.at + 24)]).ok()?;
        if self.p.read::<u64>(buf.at + 24).unwrap_or(0) != 0 {
            return None;
        }
        Some(r)
    }

    /// Calls a static method taking a string key and, for setters, a float, as PlayerPrefs.GetFloat(key)
    /// and SetFloat(key, value) do. Returns what invoke_raw returns.
    pub fn invoke_key(&mut self, class: u64, method: &str, key: &str, value: Option<f32>) -> Option<u64> {
        let argc = 1 + value.is_some() as u64;
        let m = self.call("mono_class_get_method_from_name", &[Arg::N(class), Arg::S(method), Arg::N(argc)]).ok().filter(|&m| m != 0)?;
        let s = self.call("mono_string_new", &[Arg::N(self.domain), Arg::S(key)]).ok().filter(|&s| s != 0)?;
        // params: [the string, a pointer to the float], the float, then the exception slot.
        let buf = Buf::new(&self.p, 32)?;
        self.p.write(buf.at, s);
        if let Some(v) = value {
            self.p.write(buf.at + 16, v.to_bits());
            self.p.write(buf.at + 8, buf.at + 16);
        }
        let r = self.call("mono_runtime_invoke", &[Arg::N(m), Arg::N(0), Arg::N(buf.at), Arg::N(buf.at + 24)]).ok()?;
        if self.p.read::<u64>(buf.at + 24).unwrap_or(0) != 0 {
            return None;
        }
        Some(r)
    }

    /// The class's vtable in the game's domain (also the first word of every instance).
    pub fn vtable(&self, class: u64) -> Option<u64> {
        self.call("mono_class_vtable", &[Arg::N(self.domain), Arg::N(class)]).ok().filter(|&v| v != 0)
    }

    /// Where a class's static fields live.
    pub fn static_data(&mut self, class: u64) -> Option<u64> {
        if let Some(&d) = self.statics.get(&class) {
            return Some(d);
        }
        let vt = self.vtable(class)?;
        let d = self.call("mono_vtable_get_static_field_data", &[Arg::N(vt)]).ok().filter(|&d| d != 0)?;
        self.statics.insert(class, d);
        Some(d)
    }

    /// Address of a static field.
    pub fn static_addr(&mut self, class: u64, name: &str) -> Option<u64> {
        let off = self.field(class, name)? as u64;
        Some(self.static_data(class)? + off)
    }

    /// Every live object of `class`, found by its vtable pointer in the game's heap memory.
    pub fn instances(&self, class: u64, limit: usize) -> Vec<u64> {
        self.instances_of(&[class], limit).pop().unwrap_or_default()
    }

    /// Instances of several classes in one pass over the heap (each class up to `limit`).
    pub fn instances_of(&self, classes: &[u64], limit: usize) -> Vec<Vec<u64>> {
        let vts: Vec<u64> = classes.iter().map(|&c| self.vtable(c).unwrap_or(0)).collect();
        remote::find_words(&self.p, &vts, limit)
    }

    /// Dev aid: every field of a class as (name, offset, type name).
    pub fn fields_of(&self, class: u64) -> Vec<(String, u32, String)> {
        let Some(iter) = Buf::new(&self.p, 16) else { return vec![] };
        let mut out = Vec::new();
        for _ in 0..500 {
            let Ok(f) = self.call("mono_class_get_fields", &[Arg::N(class), Arg::N(iter.at)]) else { break };
            if f == 0 {
                break;
            }
            let name = self.call("mono_field_get_name", &[Arg::N(f)]).ok().map(|s| self.cstr(s)).unwrap_or_default();
            let off = self.call("mono_field_get_offset", &[Arg::N(f)]).unwrap_or(0) as u32;
            let ty = self.call("mono_field_get_type", &[Arg::N(f)]).ok()
                .and_then(|t| self.call("mono_type_get_name", &[Arg::N(t)]).ok())
                .map(|s| self.cstr(s)).unwrap_or_default();
            out.push((name, off, ty));
        }
        out
    }

    /// Dev aid: the constants of an enum class, as (name, value).
    pub fn enum_values(&self, class: u64) -> Vec<(String, i64)> {
        let Some(vt) = self.vtable(class) else { return vec![] };
        let Some(buf) = Buf::new(&self.p, 16) else { return vec![] };
        let mut out = Vec::new();
        for (name, _, _) in self.fields_of(class) {
            if name == "value__" {
                continue;
            }
            let Ok(f) = self.call("mono_class_get_field_from_name", &[Arg::N(class), Arg::S(&name)]) else { continue };
            self.p.write(buf.at, 0u64);
            if self.call("mono_field_static_get_value", &[Arg::N(vt), Arg::N(f), Arg::N(buf.at)]).is_ok() {
                out.push((name, self.p.read::<i32>(buf.at).unwrap_or(0) as i64));
            }
        }
        out
    }

    /// Dev aid: every class of the game's own code (Assembly-CSharp), as names.
    pub fn game_classes(&self) -> Vec<String> {
        // Metadata table 2 is TypeDef; class tokens are 0x02000000 + row.
        let n = self.call("mono_image_get_table_rows", &[Arg::N(self.image), Arg::N(2)]).unwrap_or(0).min(50_000);
        (1..=n).filter_map(|i| self.call("mono_class_get", &[Arg::N(self.image), Arg::N(0x0200_0000 | i)]).ok().filter(|&c| c != 0)).map(|c| self.class_name(c)).collect()
    }

    /// Dev aid: the name of a class.
    pub fn class_name(&self, class: u64) -> String {
        let ns = self.call("mono_class_get_namespace", &[Arg::N(class)]).ok().map(|s| self.cstr(s)).unwrap_or_default();
        let name = self.call("mono_class_get_name", &[Arg::N(class)]).ok().map(|s| self.cstr(s)).unwrap_or_default();
        if ns.is_empty() { name } else { format!("{ns}.{name}") }
    }

    /// A managed System.String (length at +0x10, UTF-16 characters from +0x14).
    pub fn string(&self, s: u64) -> String {
        let len = self.p.read::<i32>(s + 0x10).unwrap_or(0).clamp(0, 256) as usize;
        let mut b = vec![0u8; len * 2];
        self.p.read_into(s + 0x14, &mut b);
        String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
    }

    pub fn cstr(&self, addr: u64) -> String {
        remote::cstr(&self.p, addr)
    }
}

impl Mono {
    /// World position of a UnityEngine.Transform (managed object), read from Unity's native
    /// transform hierarchy: the managed object points to the native Transform (m_CachedPtr at
    /// +0x10), which holds its hierarchy and index; positions are local, so walk up the parents.
    pub fn transform_pos(&self, transform: u64) -> Option<[f32; 3]> {
        self.transform_world(transform).map(|(p, _)| p)
    }

    /// World position and rotation (quaternion x, y, z, w) of a Transform.
    pub fn transform_world(&self, transform: u64) -> Option<([f32; 3], [f32; 4])> {
        let native = self.p.ptr(transform + 0x10)?;
        let hierarchy = self.p.ptr(native + 0x38)?;
        let mut index = self.p.read::<i32>(native + 0x40)?;
        let locals = self.p.ptr(hierarchy + 0x18)?;
        let parents = self.p.ptr(hierarchy + 0x20)?;
        let read = |i: i32| -> Option<[f32; 12]> {
            let mut b = [0u8; 48];
            self.p.read_into(locals + i as u64 * 48, &mut b).then_some(())?;
            Some(std::array::from_fn(|k| f32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().unwrap())))
        };
        let t = read(index)?;
        let mut pos = [t[0], t[1], t[2]];
        let mut rot = [t[4], t[5], t[6], t[7]];
        for _ in 0..64 {
            index = self.p.read::<i32>(parents + index as u64 * 4)?;
            if index < 0 {
                break;
            }
            // Parent: translation (0..3), rotation quaternion xyzw (4..8), scale (8..11).
            let p = read(index)?;
            let s = [pos[0] * p[8], pos[1] * p[9], pos[2] * p[10]];
            let (qx, qy, qz, qw) = (p[4], p[5], p[6], p[7]);
            // v' = v + 2w(q x v) + 2 q x (q x v)
            let c1 = [qy * s[2] - qz * s[1], qz * s[0] - qx * s[2], qx * s[1] - qy * s[0]];
            let c2 = [qy * c1[2] - qz * c1[1], qz * c1[0] - qx * c1[2], qx * c1[1] - qy * c1[0]];
            pos = [0, 1, 2].map(|k| s[k] + 2.0 * (qw * c1[k] + c2[k]) + p[k]);
            // rot = parent * rot
            let [x, y, z, w] = rot;
            rot = [
                qw * x + qx * w + qy * z - qz * y,
                qw * y - qx * z + qy * w + qz * x,
                qw * z + qx * y - qy * x + qz * w,
                qw * w - qx * x - qy * y - qz * z,
            ];
        }
        pos.iter().all(|v| v.is_finite() && v.abs() < 1e6).then_some((pos, rot))
    }
}
