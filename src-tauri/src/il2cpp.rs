//! Reads Unity games built with IL2CPP (GameAssembly.dll), the same way mono.rs reads Mono games:
//! the runtime's exported API (il2cpp_class_from_name, il2cpp_class_get_field_from_name, ...) is
//! called inside the game, so field offsets and classes come from the game itself and survive
//! updates. Objects start with their class pointer, so live objects are found by that.

use crate::remote::{self, Arg, Attach, Buf};
use crate::ue::mem::Proc;
use std::collections::HashMap;

pub struct Il2Cpp {
    pub p: Proc,
    exports: HashMap<String, u64>,
    domain: u64,
    /// Every loaded assembly's image, Assembly-CSharp first.
    images: Vec<u64>,
    classes: HashMap<String, Option<u64>>,
    statics: HashMap<u64, Option<u64>>,
}

/// Where Il2CppClass keeps its static fields in Unity 2018 to 6 (checked before use).
const STATIC_FIELDS: u64 = 0xB8;

impl Il2Cpp {
    pub fn attach(pid: u32) -> Result<Il2Cpp, String> {
        let p = Proc::open(pid)?;
        let (base, _) = p.module("GameAssembly.dll").ok_or("GameAssembly.dll isn't loaded (not an IL2CPP Unity game?)")?;
        let exports = remote::exports(&p, base);
        if !exports.contains_key("il2cpp_class_from_name") {
            return Err("IL2CPP API not found (the game hides it)".into());
        }
        let mut m = Il2Cpp { p, exports, domain: 0, images: Vec::new(), classes: HashMap::new(), statics: HashMap::new() };
        m.domain = m.call("il2cpp_domain_get", &[])?;
        if m.domain == 0 {
            return Err("IL2CPP hasn't started yet".into());
        }
        let size = Buf::new(&m.p, 16).ok_or("couldn't allocate memory in the game")?;
        let list = m.call("il2cpp_domain_get_assemblies", &[Arg::N(m.domain), Arg::N(size.at)])?;
        let n = m.p.read::<u64>(size.at).unwrap_or(0).min(4096);
        drop(size);
        for i in 0..n {
            let Some(asm) = m.p.ptr(list + i * 8) else { continue };
            let Ok(img) = m.call("il2cpp_assembly_get_image", &[Arg::N(asm)]) else { continue };
            if img == 0 {
                continue;
            }
            let name = m.call("il2cpp_image_get_name", &[Arg::N(img)]).map(|s| remote::cstr(&m.p, s)).unwrap_or_default();
            if name.starts_with("Assembly-CSharp.") {
                m.images.insert(0, img);
            } else {
                m.images.push(img);
            }
        }
        if m.images.is_empty() {
            return Err("the game's code isn't loaded yet".into());
        }
        Ok(m)
    }

    pub fn call(&self, func: &str, args: &[Arg]) -> Result<u64, String> {
        let f = *self.exports.get(func).ok_or_else(|| format!("{func} not exported"))?;
        let attach = (self.domain != 0).then(|| Attach { attach: self.exports["il2cpp_thread_attach"], detach: self.exports["il2cpp_thread_detach"], domain: self.domain });
        remote::call(&self.p, f, args, attach)
    }

    pub fn class(&mut self, ns: &str, name: &str) -> Option<u64> {
        let key = format!("{ns}.{name}");
        if let Some(&c) = self.classes.get(&key) {
            return c;
        }
        let c = self.images.iter().find_map(|&img| self.call("il2cpp_class_from_name", &[Arg::N(img), Arg::S(ns), Arg::S(name)]).ok().filter(|&c| c != 0));
        self.classes.insert(key, c);
        c
    }

    /// A field's offset, type code (Il2CppTypeEnum, same numbers as Mono's) and whether it is static.
    pub fn field_info(&mut self, class: u64, name: &str) -> Option<(u32, u8, bool)> {
        let f = self.call("il2cpp_class_get_field_from_name", &[Arg::N(class), Arg::S(name)]).ok().filter(|&f| f != 0)?;
        let off = self.call("il2cpp_field_get_offset", &[Arg::N(f)]).ok()? as u32;
        let ty = self.call("il2cpp_field_get_type", &[Arg::N(f)]).ok().filter(|&t| t != 0)?;
        let code = self.call("il2cpp_type_get_type", &[Arg::N(ty)]).ok()? as u8;
        let flags = self.call("il2cpp_field_get_flags", &[Arg::N(f)]).unwrap_or(0);
        Some((off, code, flags & 0x10 != 0))
    }

    pub fn class_of(&self, obj: u64) -> Option<u64> {
        self.p.ptr(obj)
    }

    /// Where a class's static fields live. Il2CppClass isn't exported, so the usual spot is checked
    /// against il2cpp_field_static_get_value for one of the class's static fields.
    pub fn static_data(&mut self, class: u64) -> Option<u64> {
        if let Some(&d) = self.statics.get(&class) {
            return d;
        }
        let _ = self.call("il2cpp_runtime_class_init", &[Arg::N(class)]);
        let d = self.p.ptr(class + STATIC_FIELDS);
        let ok = d.is_some_and(|d| self.check_statics(class, d));
        let d = if ok {
            d
        } else {
            // Another Unity version: look for the pointer that agrees with the runtime.
            (0x60..0x140).step_by(8).filter_map(|o| self.p.ptr(class + o)).find(|&d| self.check_statics(class, d))
        };
        self.statics.insert(class, d);
        d
    }

    /// True if the static fields read from `data` match what the runtime says they are.
    fn check_statics(&self, class: u64, data: u64) -> bool {
        let Some(iter) = Buf::new(&self.p, 32) else { return false };
        let (mut checked, mut agree) = (0, 0);
        for _ in 0..200 {
            let Ok(f) = self.call("il2cpp_class_get_fields", &[Arg::N(class), Arg::N(iter.at)]) else { break };
            if f == 0 || checked >= 3 {
                break;
            }
            if self.call("il2cpp_field_get_flags", &[Arg::N(f)]).unwrap_or(0) & 0x10 == 0 {
                continue;
            }
            let off = self.call("il2cpp_field_get_offset", &[Arg::N(f)]).unwrap_or(0) as u64;
            // Thread-static and literal fields have no slot.
            if off as u32 == u32::MAX || self.call("il2cpp_field_get_flags", &[Arg::N(f)]).unwrap_or(0) & 0x40 != 0 {
                continue;
            }
            self.p.write(iter.at + 16, 0u64);
            if self.call("il2cpp_field_static_get_value", &[Arg::N(f), Arg::N(iter.at + 16)]).is_err() {
                continue;
            }
            checked += 1;
            let want = self.p.read::<u64>(iter.at + 16).unwrap_or(0);
            // Compare the first 4 bytes: smaller fields leave the rest of our buffer alone.
            if self.p.read::<u32>(data + off) == Some(want as u32) {
                agree += 1;
            }
        }
        checked > 0 && agree == checked
    }

    /// Calls a method with one 4-byte argument (a float's or an int's bits) or none, and returns what
    /// it returned: an object, or a boxed value (its data at +0x10).
    pub fn invoke_raw(&mut self, class: u64, obj: u64, method: &str, arg: Option<u32>) -> Option<u64> {
        let m = self.call("il2cpp_class_get_method_from_name", &[Arg::N(class), Arg::S(method), Arg::N(arg.is_some() as u64)]).ok().filter(|&m| m != 0)?;
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
        let r = self.call("il2cpp_runtime_invoke", &[Arg::N(m), Arg::N(obj), Arg::N(params), Arg::N(buf.at + 24)]).ok()?;
        if self.p.read::<u64>(buf.at + 24).unwrap_or(0) != 0 {
            return None;
        }
        Some(r)
    }

    /// Dev aid: every field of a class as (name, offset, type name).
    pub fn fields_of(&self, class: u64) -> Vec<(String, u32, String)> {
        let Some(iter) = Buf::new(&self.p, 16) else { return vec![] };
        let mut out = Vec::new();
        for _ in 0..500 {
            let Ok(f) = self.call("il2cpp_class_get_fields", &[Arg::N(class), Arg::N(iter.at)]) else { break };
            if f == 0 {
                break;
            }
            let name = self.call("il2cpp_field_get_name", &[Arg::N(f)]).map(|s| remote::cstr(&self.p, s)).unwrap_or_default();
            let off = self.call("il2cpp_field_get_offset", &[Arg::N(f)]).unwrap_or(0) as u32;
            let ty = self.call("il2cpp_field_get_type", &[Arg::N(f)]).ok()
                .and_then(|t| self.call("il2cpp_type_get_name", &[Arg::N(t)]).ok())
                .map(|s| remote::cstr(&self.p, s)).unwrap_or_default();
            out.push((name, off, ty));
        }
        out
    }

    pub fn class_name(&self, class: u64) -> String {
        let ns = self.call("il2cpp_class_get_namespace", &[Arg::N(class)]).map(|s| remote::cstr(&self.p, s)).unwrap_or_default();
        let name = self.call("il2cpp_class_get_name", &[Arg::N(class)]).map(|s| remote::cstr(&self.p, s)).unwrap_or_default();
        if ns.is_empty() { name } else { format!("{ns}.{name}") }
    }

    /// Dev aid: every class of the game's own code (Assembly-CSharp), as names.
    pub fn game_classes(&self) -> Vec<String> {
        let Some(&img) = self.images.first() else { return vec![] };
        let n = self.call("il2cpp_image_get_class_count", &[Arg::N(img)]).unwrap_or(0).min(50_000);
        (0..n).filter_map(|i| self.call("il2cpp_image_get_class", &[Arg::N(img), Arg::N(i)]).ok().filter(|&c| c != 0)).map(|c| self.class_name(c)).collect()
    }
}
