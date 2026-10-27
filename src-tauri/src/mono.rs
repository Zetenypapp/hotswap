//! Reads Unity games that run on Mono. Instead of guessing Mono's internal layouts (they change
//! between Unity versions), Hotswap asks Mono itself: it calls the runtime's exported API
//! (mono_class_from_name, mono_class_get_field_from_name, ...) from a short-lived thread in the
//! game, attached to Mono for the call. The answers (field offsets, static field addresses, class
//! vtables) are cached, and the mods themselves are plain memory reads and writes.

use crate::ue::mem::Proc;
use std::collections::HashMap;
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::{
        Memory::{VirtualAllocEx, VirtualFreeEx, VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_PRIVATE, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE, PAGE_READWRITE},
        Threading::{CreateRemoteThread, WaitForSingleObject},
    },
};

pub struct Mono {
    pub p: Proc,
    exports: HashMap<String, u64>,
    domain: u64,
    /// Assembly-CSharp, where a Unity game's own classes live.
    image: u64,
    classes: HashMap<String, u64>,
    fields: HashMap<(u64, String), Option<u32>>,
    statics: HashMap<u64, u64>,
}

/// An argument for a call into the game: a number, or a string copied into the game's memory.
pub enum Arg<'a> {
    N(u64),
    S(&'a str),
}

impl Mono {
    pub fn attach(pid: u32) -> Result<Mono, String> {
        let p = Proc::open(pid)?;
        let (base, _) = p.module("mono-2.0-bdwgc.dll").or_else(|| p.module("mono.dll")).ok_or("Mono isn't loaded (not a Mono Unity game?)")?;
        let exports = exports(&p, base);
        if !exports.contains_key("mono_class_from_name") {
            return Err("Mono API not found".into());
        }
        let mut m = Mono { p, exports, domain: 0, image: 0, classes: HashMap::new(), fields: HashMap::new(), statics: HashMap::new() };
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
        let h = self.p.raw_handle();
        let mem = unsafe { VirtualAllocEx(h, std::ptr::null(), 0x1000, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) } as u64;
        if mem == 0 {
            return Err("couldn't allocate memory in the game".into());
        }
        let (result, thread_slot, strings) = (mem + 0x400, mem + 0x408, mem + 0x500);
        // Strings go after the code; numbers are used as is.
        let mut values = [0u64; 4];
        let mut next = strings;
        for (i, a) in args.iter().take(4).enumerate() {
            values[i] = match a {
                Arg::N(n) => *n,
                Arg::S(s) => {
                    let mut bytes = s.as_bytes().to_vec();
                    bytes.push(0);
                    if !self.p.write_bytes(next, &bytes) {
                        return Err("couldn't write a string".into());
                    }
                    let at = next;
                    next += (bytes.len() as u64 + 15) & !15;
                    at
                }
            };
        }
        let mut code: Vec<u8> = vec![0x48, 0x83, 0xEC, 0x28]; // sub rsp, 0x28
        let attach = self.domain != 0 && func != "mono_get_root_domain";
        let mov = |code: &mut Vec<u8>, op: [u8; 2], v: u64| { code.extend(op); code.extend(v.to_le_bytes()); };
        if attach {
            // mono_thread_attach(domain): Mono only allows calls from threads it knows about.
            mov(&mut code, [0x48, 0xB9], self.domain); // mov rcx, domain
            mov(&mut code, [0x48, 0xB8], self.exports["mono_thread_attach"]); // mov rax, fn
            code.extend([0xFF, 0xD0]); // call rax
            mov(&mut code, [0x48, 0xA3], thread_slot); // mov [thread_slot], rax
        }
        mov(&mut code, [0x48, 0xB9], values[0]); // rcx
        mov(&mut code, [0x48, 0xBA], values[1]); // rdx
        mov(&mut code, [0x49, 0xB8], values[2]); // r8
        mov(&mut code, [0x49, 0xB9], values[3]); // r9
        mov(&mut code, [0x48, 0xB8], f);
        code.extend([0xFF, 0xD0]); // call rax
        mov(&mut code, [0x48, 0xA3], result); // mov [result], rax
        if attach {
            mov(&mut code, [0x48, 0xB8], thread_slot); // mov rax, &thread
            code.extend([0x48, 0x8B, 0x08]); // mov rcx, [rax]
            mov(&mut code, [0x48, 0xB8], self.exports["mono_thread_detach"]);
            code.extend([0xFF, 0xD0]); // call rax
        }
        code.extend([0x48, 0x83, 0xC4, 0x28, 0x31, 0xC0, 0xC3]); // add rsp, 0x28; xor eax, eax; ret
        let out = if !self.p.write_bytes(mem, &code) {
            Err("couldn't write the call stub".to_string())
        } else {
            let thread = unsafe { CreateRemoteThread(h, std::ptr::null(), 0, Some(std::mem::transmute(mem as usize)), std::ptr::null(), 0, std::ptr::null_mut()) };
            if thread.is_null() {
                Err(format!("couldn't start the call ({})", std::io::Error::last_os_error()))
            } else {
                let waited = unsafe { WaitForSingleObject(thread, 3000) };
                unsafe { CloseHandle(thread) };
                if waited != 0 {
                    return Err("the game didn't finish the call in time".into());
                }
                self.p.read::<u64>(result).ok_or_else(|| "couldn't read the result".into())
            }
        };
        unsafe { VirtualFreeEx(h, mem as _, 0, MEM_RELEASE) };
        out
    }

    /// A class from the game's code, e.g. ("", "Player").
    pub fn class(&mut self, ns: &str, name: &str) -> Option<u64> {
        let key = format!("{ns}.{name}");
        if let Some(&c) = self.classes.get(&key) {
            return Some(c);
        }
        let c = self.call("mono_class_from_name", &[Arg::N(self.image), Arg::S(ns), Arg::S(name)]).ok().filter(|&c| c != 0)?;
        self.classes.insert(key, c);
        Some(c)
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

    /// The class's vtable in the game's domain (also the first word of every instance).
    pub fn vtable(&self, class: u64) -> Option<u64> {
        self.call("mono_class_vtable", &[Arg::N(self.domain), Arg::N(class)]).ok().filter(|&v| v != 0)
    }

    /// Address of a static field.
    pub fn static_addr(&mut self, class: u64, name: &str) -> Option<u64> {
        let off = self.field(class, name)? as u64;
        let data = match self.statics.get(&class) {
            Some(&d) => d,
            None => {
                let vt = self.vtable(class)?;
                let d = self.call("mono_vtable_get_static_field_data", &[Arg::N(vt)]).ok().filter(|&d| d != 0)?;
                self.statics.insert(class, d);
                d
            }
        };
        Some(data + off)
    }

    /// Every live object of `class`, found by its vtable pointer in the game's heap memory.
    pub fn instances(&self, class: u64, limit: usize) -> Vec<u64> {
        let Some(vt) = self.vtable(class) else { return vec![] };
        let mut out = Vec::new();
        for (base, size) in heap_regions(&self.p) {
            let mut buf = vec![0u8; size];
            if !self.p.read_into(base, &mut buf) {
                continue;
            }
            for (i, w) in buf.chunks_exact(8).enumerate() {
                if u64::from_le_bytes(w.try_into().unwrap()) == vt {
                    out.push(base + i as u64 * 8);
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
        }
        out
    }

    /// Dev aid: every field of a class as (name, offset, type name).
    pub fn fields_of(&self, class: u64) -> Vec<(String, u32, String)> {
        let h = self.p.raw_handle();
        let iter = unsafe { VirtualAllocEx(h, std::ptr::null(), 16, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) } as u64;
        let mut out = Vec::new();
        for _ in 0..500 {
            let Ok(f) = self.call("mono_class_get_fields", &[Arg::N(class), Arg::N(iter)]) else { break };
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
        unsafe { VirtualFreeEx(h, iter as _, 0, MEM_RELEASE) };
        out
    }

    /// Dev aid: the constants of an enum class, as (name, value).
    pub fn enum_values(&self, class: u64) -> Vec<(String, i64)> {
        let Some(vt) = self.vtable(class) else { return vec![] };
        let h = self.p.raw_handle();
        let buf = unsafe { VirtualAllocEx(h, std::ptr::null(), 16, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) } as u64;
        let mut out = Vec::new();
        for (name, _, _) in self.fields_of(class) {
            if name == "value__" {
                continue;
            }
            let Ok(f) = self.call("mono_class_get_field_from_name", &[Arg::N(class), Arg::S(&name)]) else { continue };
            self.p.write(buf, 0u64);
            if self.call("mono_field_static_get_value", &[Arg::N(vt), Arg::N(f), Arg::N(buf)]).is_ok() {
                out.push((name, self.p.read::<i32>(buf).unwrap_or(0) as i64));
            }
        }
        unsafe { VirtualFreeEx(h, buf as _, 0, MEM_RELEASE) };
        out
    }

    pub fn cstr(&self, addr: u64) -> String {
        let mut b = [0u8; 256];
        self.p.read_into(addr, &mut b);
        String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(256)]).to_string()
    }
}

/// Exported functions of the module loaded at `base`: name -> address.
fn exports(p: &Proc, base: u64) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    let Some(pe) = p.read::<u32>(base + 0x3C).map(|o| base + o as u64) else { return out };
    let Some(dir) = p.read::<u32>(pe + 0x88).filter(|&d| d != 0).map(|d| base + d as u64) else { return out };
    let n = p.read::<u32>(dir + 0x18).unwrap_or(0) as u64;
    let [funcs, names, ords] = [0x1C, 0x20, 0x24].map(|o| base + p.read::<u32>(dir + o).unwrap_or(0) as u64);
    for i in 0..n.min(20000) {
        let (Some(name_rva), Some(ord)) = (p.read::<u32>(names + i * 4), p.read::<u16>(ords + i * 2)) else { continue };
        let Some(rva) = p.read::<u32>(funcs + ord as u64 * 4) else { continue };
        let mut b = [0u8; 96];
        p.read_into(base + name_rva as u64, &mut b);
        let name = String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(96)]).to_string();
        out.insert(name, base + rva as u64);
    }
    out
}

/// Committed, private, read-write memory: where Mono's heap lives.
fn heap_regions(p: &Proc) -> Vec<(u64, usize)> {
    let mut out = Vec::new();
    let mut addr = 0x10000u64;
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    while addr < 0x7FFF_FFFF_0000 {
        let got = unsafe { VirtualQueryEx(p.raw_handle(), addr as _, &mut info, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) };
        if got == 0 {
            break;
        }
        let (base, size) = (info.BaseAddress as u64, info.RegionSize as u64);
        if info.State == MEM_COMMIT && info.Type == MEM_PRIVATE && info.Protect == PAGE_READWRITE && size < (512 << 20) {
            out.push((base, size as usize));
        }
        addr = base + size;
    }
    out
}
