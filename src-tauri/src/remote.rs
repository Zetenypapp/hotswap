//! Calling functions inside a running game: a short stub is written into the game's memory and run
//! on a new thread there. Used by the Mono and IL2CPP readers to ask the runtime itself where
//! classes and fields are, so nothing depends on a game version's memory layout.

use crate::ue::mem::Proc;
use std::collections::HashMap;
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::{
        Memory::{VirtualAllocEx, VirtualFreeEx, VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_PRIVATE, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE, PAGE_READWRITE},
        Threading::{CreateRemoteThread, WaitForSingleObject},
    },
};

/// An argument for a call into the game: a number, or a string copied into the game's memory.
pub enum Arg<'a> {
    N(u64),
    S(&'a str),
}

/// A runtime that only takes calls from threads it knows: `attach(domain)` before the call,
/// `detach(thread)` after.
#[derive(Clone, Copy)]
pub struct Attach {
    pub attach: u64,
    pub detach: u64,
    pub domain: u64,
}

/// Calls the function at `f` with up to 4 arguments and returns what it returned.
pub fn call(p: &Proc, f: u64, args: &[Arg], attach: Option<Attach>) -> Result<u64, String> {
    let h = p.raw_handle();
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
                if next + bytes.len() as u64 > mem + 0x1000 || !p.write_bytes(next, &bytes) {
                    unsafe { VirtualFreeEx(h, mem as _, 0, MEM_RELEASE) };
                    return Err("couldn't write a string".into());
                }
                let at = next;
                next += (bytes.len() as u64 + 15) & !15;
                at
            }
        };
    }
    let mut code: Vec<u8> = vec![0x48, 0x83, 0xEC, 0x28]; // sub rsp, 0x28
    let mov = |code: &mut Vec<u8>, op: [u8; 2], v: u64| { code.extend(op); code.extend(v.to_le_bytes()); };
    if let Some(a) = attach {
        mov(&mut code, [0x48, 0xB9], a.domain); // mov rcx, domain
        mov(&mut code, [0x48, 0xB8], a.attach); // mov rax, fn
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
    if let Some(a) = attach {
        mov(&mut code, [0x48, 0xB8], thread_slot); // mov rax, &thread
        code.extend([0x48, 0x8B, 0x08]); // mov rcx, [rax]
        mov(&mut code, [0x48, 0xB8], a.detach);
        code.extend([0xFF, 0xD0]); // call rax
    }
    code.extend([0x48, 0x83, 0xC4, 0x28, 0x31, 0xC0, 0xC3]); // add rsp, 0x28; xor eax, eax; ret
    let out = if !p.write_bytes(mem, &code) {
        Err("couldn't write the call stub".to_string())
    } else {
        let thread = unsafe { CreateRemoteThread(h, std::ptr::null(), 0, Some(std::mem::transmute(mem as usize)), std::ptr::null(), 0, std::ptr::null_mut()) };
        if thread.is_null() {
            Err(format!("couldn't start the call ({})", std::io::Error::last_os_error()))
        } else {
            let waited = unsafe { WaitForSingleObject(thread, 3000) };
            unsafe { CloseHandle(thread) };
            if waited != 0 {
                // The stub may still run: leave its memory alone.
                return Err("the game didn't finish the call in time".into());
            }
            p.read::<u64>(result).ok_or_else(|| "couldn't read the result".into())
        }
    };
    unsafe { VirtualFreeEx(h, mem as _, 0, MEM_RELEASE) };
    out
}

/// A small read-write buffer in the game, freed when dropped.
pub struct Buf<'a> {
    p: &'a Proc,
    pub at: u64,
}

impl<'a> Buf<'a> {
    pub fn new(p: &'a Proc, size: usize) -> Option<Buf<'a>> {
        let at = unsafe { VirtualAllocEx(p.raw_handle(), std::ptr::null(), size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) } as u64;
        (at != 0).then_some(Buf { p, at })
    }

    /// Memory the game can also run code from.
    pub fn exec(p: &'a Proc, size: usize) -> Option<Buf<'a>> {
        let at = unsafe { VirtualAllocEx(p.raw_handle(), std::ptr::null(), size, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) } as u64;
        (at != 0).then_some(Buf { p, at })
    }
}

impl Drop for Buf<'_> {
    fn drop(&mut self) {
        unsafe { VirtualFreeEx(self.p.raw_handle(), self.at as _, 0, MEM_RELEASE) };
    }
}

/// Exported functions of the module loaded at `base`: name -> address.
pub fn exports(p: &Proc, base: u64) -> HashMap<String, u64> {
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

/// Committed, private, read-write memory: where managed heaps live.
pub fn heap_regions(p: &Proc) -> Vec<(u64, usize)> {
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

/// Addresses of words equal to any of `wanted` in the heap, each up to `limit` hits (0s are skipped).
pub fn find_words(p: &Proc, wanted: &[u64], limit: usize) -> Vec<Vec<u64>> {
    let mut out = vec![Vec::new(); wanted.len()];
    if wanted.iter().all(|&v| v == 0) {
        return out;
    }
    // Nearly every word is outside the range the wanted values span: one compare skips it.
    let (lo, hi) = wanted.iter().filter(|&&v| v != 0).fold((u64::MAX, 0), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    // One reused 4 MB buffer: regions can be hundreds of MB, and a page that can't be read
    // only costs its own chunk.
    const CHUNK: usize = 4 << 20;
    let mut buf = vec![0u8; CHUNK];
    for (base, size) in heap_regions(p) {
        for off in (0..size).step_by(CHUNK) {
            let n = CHUNK.min(size - off);
            let at = base + off as u64;
            if !p.read_into(at, &mut buf[..n]) {
                continue;
            }
            for (i, w) in buf[..n].chunks_exact(8).enumerate() {
                let w = u64::from_le_bytes(w.try_into().unwrap());
                if w < lo || w > hi {
                    continue;
                }
                for (k, v) in wanted.iter().enumerate() {
                    if *v == w && out[k].len() < limit {
                        out[k].push(at + i as u64 * 8);
                    }
                }
            }
            if out.iter().zip(wanted).all(|(o, &v)| v == 0 || o.len() >= limit) {
                return out;
            }
        }
    }
    out
}

pub fn cstr(p: &Proc, addr: u64) -> String {
    let mut b = [0u8; 256];
    p.read_into(addr, &mut b);
    String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(256)]).to_string()
}
