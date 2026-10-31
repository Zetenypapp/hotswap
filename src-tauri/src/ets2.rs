//! Euro Truck Simulator 2 (Steam app 227300) and American Truck Simulator (270880), live.
//!
//! No offsets are hard-coded. SCS's engine keeps the names, offsets and types of every field its
//! save files use inside the game's exe (its serializer's metadata); `Layout` reads them from the
//! exe that's running, so a game update is followed automatically. The live objects are then found
//! by their class's vtable: the economy (money, XP, the player and their truck) through a pointer
//! in the exe's data, and the damage settings through the code that reads them.
//! Every mod writes plain game values; nothing in the game's code is changed.
//!
//! Ported from ETS2Trainer by Berke Akçen (Despical), GPL-3.0-or-later:
//! https://github.com/Despical/ETS2Trainer

use crate::{trainer::log, ue::mem::Proc};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub const ETS2: &str = "227300";
pub const ATS: &str = "270880";

/// Field types in the metadata.
const FLOAT: u64 = 5;
const FLOAT_ARRAY: u64 = 6;

struct Section {
    rva: u64,
    off: usize,
    len: usize,
    vlen: usize,
    flags: u32,
}
impl Section {
    fn exec(&self) -> bool {
        self.flags & 0x2000_0000 != 0
    }
    fn writable(&self) -> bool {
        self.flags & 0x8000_0000 != 0
    }
}

#[derive(Clone)]
pub struct Class {
    pub name: String,
    addr: u64,
    /// field -> (offset, type)
    pub fields: HashMap<String, (u64, u64)>,
    pub vtable: u64,
}
impl Class {
    fn f(&self, name: &str) -> Option<u64> {
        self.fields.get(name).map(|f| f.0)
    }
    fn typed(&self, name: &str, kind: u64) -> Option<u64> {
        self.fields.get(name).filter(|f| f.1 == kind).map(|f| f.0)
    }
}

/// The game's own description of its save-file classes, read from the exe on disk.
pub struct Layout {
    image: Vec<u8>,
    base: u64,
    size: u64,
    sections: Vec<Section>,
    classes: HashMap<u64, Class>,
    economy: Class,
    bank: Class,
    player: Class,
    vehicle: Class,
    player_vehicles: Option<Class>,
    trailer: Option<Class>,
    damage: Option<Class>,
    /// Where the active damage settings are: a pointer in the exe's data, then a member of it.
    damage_root: Option<(u64, u64)>,
}

fn u16_at(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes(b[p..p + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn i32_at(b: &[u8], p: usize) -> i32 {
    i32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], p: usize) -> u64 {
    u64::from_le_bytes(b[p..p + 8].try_into().unwrap())
}

/// Every position of `needle` in `hay`.
fn find_all(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let (first, rest) = (needle[0], &needle[1..]);
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        match hay[i..hay.len() - needle.len() + 1].iter().position(|&b| b == first) {
            None => break,
            Some(k) => {
                let at = i + k;
                if &hay[at + 1..at + needle.len()] == rest {
                    out.push(at);
                }
                i = at + 1;
            }
        }
    }
    out
}

impl Layout {
    pub fn read(path: &std::path::Path) -> Result<Layout, String> {
        let image = std::fs::read(path).map_err(|e| format!("Couldn't read the game's exe: {e}"))?;
        if image.len() < 4096 {
            return Err("The game's exe looks wrong".into());
        }
        let pe = i32_at(&image, 0x3c) as usize;
        if u32_at(&image, pe) != 0x4550 || u16_at(&image, pe + 24) != 0x20b {
            return Err("Needs the 64-bit game".into());
        }
        let base = u64_at(&image, pe + 48);
        let size = u32_at(&image, pe + 80) as u64;
        let (count, table) = (u16_at(&image, pe + 6) as usize, pe + 24 + u16_at(&image, pe + 20) as usize);
        let mut sections = Vec::new();
        for i in 0..count {
            let p = table + i * 40;
            let s = Section {
                rva: u32_at(&image, p + 12) as u64,
                off: u32_at(&image, p + 20) as usize,
                len: u32_at(&image, p + 16) as usize,
                vlen: u32_at(&image, p + 8).max(u32_at(&image, p + 16)) as usize,
                flags: u32_at(&image, p + 36),
            };
            if s.len > 0 && s.off + s.len <= image.len() {
                sections.push(s);
            }
        }
        let mut l = Layout {
            image,
            base,
            size,
            sections,
            classes: HashMap::new(),
            economy: Class { name: String::new(), addr: 0, fields: HashMap::new(), vtable: 0 },
            bank: Class { name: String::new(), addr: 0, fields: HashMap::new(), vtable: 0 },
            player: Class { name: String::new(), addr: 0, fields: HashMap::new(), vtable: 0 },
            vehicle: Class { name: String::new(), addr: 0, fields: HashMap::new(), vtable: 0 },
            player_vehicles: None,
            trailer: None,
            damage: None,
            damage_root: None,
        };
        for seed in ["experience_points", "money_account", "fuel_relative", "assigned_vehicles", "assigned_truck", "trailer_body_wear", "truck_damage_coef"] {
            l.discover(seed);
        }
        let pick = |l: &Layout, name: &str, field: &str| l.classes.values().find(|c| c.name == name && c.fields.contains_key(field)).cloned();
        let need = |c: Option<Class>, what: &str| c.ok_or(format!("This game version has no {what}"));
        l.economy = need(pick(&l, "economy", "experience_points"), "economy.experience_points")?;
        l.bank = need(pick(&l, "bank", "money_account"), "bank.money_account")?;
        l.player = need(pick(&l, "player", "assigned_truck").or_else(|| pick(&l, "player", "assigned_vehicles")), "player")?;
        l.vehicle = need(pick(&l, "vehicle", "fuel_relative"), "vehicle.fuel_relative")?;
        if !l.classes.values().any(|c| c.name == "player_vehicles") {
            l.discover("vehicle");
        }
        l.player_vehicles = l.classes.values().find(|c| c.name == "player_vehicles").cloned();
        l.trailer = pick(&l, "trailer", "trailer_body_wear");
        l.damage = pick(&l, "vehicle_damage_data", "truck_damage_coef");

        // Vtables identify live objects; one pass over the code finds every `lea rax,[rip+x]; ret`.
        let mut lea: HashMap<u64, Vec<u64>> = HashMap::new();
        for s in l.sections.iter().filter(|s| s.exec()) {
            let code = &l.image[s.off..s.off + s.len];
            for i in 0..code.len().saturating_sub(8) {
                if code[i] == 0x48 && code[i + 1] == 0x8d && code[i + 2] == 0x05 && code[i + 7] == 0xc3 {
                    let at = l.base + s.rva + i as u64;
                    let dest = (at as i64 + 7 + i32_at(code, i + 3) as i64) as u64;
                    lea.entry(dest).or_default().push(at);
                }
            }
        }
        let mut need_vt = vec![l.economy.clone(), l.bank.clone(), l.player.clone(), l.vehicle.clone()];
        need_vt.extend(l.player_vehicles.clone());
        need_vt.extend(l.trailer.clone());
        need_vt.extend(l.damage.clone());
        let mut found = HashMap::new();
        for c in &need_vt {
            found.insert(c.addr, l.vtable(c.addr, &lea));
        }
        for c in [&mut l.economy, &mut l.bank, &mut l.player, &mut l.vehicle] {
            c.vtable = found[&c.addr];
            if c.vtable == 0 {
                return Err(format!("Couldn't identify the game's {} objects", c.name));
            }
        }
        for c in [&mut l.player_vehicles, &mut l.trailer, &mut l.damage].into_iter().flatten() {
            c.vtable = found[&c.addr];
        }
        l.damage_root = l.damage.as_ref().and_then(|d| l.find_damage_root(d));
        Ok(l)
    }

    fn file_off(&self, va: u64) -> Option<usize> {
        let rva = va.checked_sub(self.base)?;
        self.sections.iter().find(|s| rva >= s.rva && rva < s.rva + s.len as u64).map(|s| s.off + (rva - s.rva) as usize)
    }
    fn va(&self, off: usize) -> Option<u64> {
        self.sections.iter().find(|s| off >= s.off && off < s.off + s.len).map(|s| self.base + s.rva + (off - s.off) as u64)
    }
    fn string(&self, va: u64) -> String {
        let Some(p) = self.file_off(va) else { return String::new() };
        let mut n = 0;
        while p + n < self.image.len() && n < 120 && self.image[p + n] != 0 {
            if !(32..=126).contains(&self.image[p + n]) {
                return String::new();
            }
            n += 1;
        }
        if n >= 120 { String::new() } else { String::from_utf8_lossy(&self.image[p..p + n]).into_owned() }
    }

    /// Finds the classes that have a field called `field`.
    fn discover(&mut self, field: &str) {
        let mut needle = field.as_bytes().to_vec();
        needle.push(0);
        for at in find_all(&self.image, &needle) {
            if at > 0 && self.image[at - 1] != 0 {
                continue;
            }
            let Some(addr) = self.va(at) else { continue };
            for p in find_all(&self.image, &addr.to_le_bytes()) {
                if p < 32 {
                    continue;
                }
                let (owner, member, kind) = (u64_at(&self.image, p - 32), u64_at(&self.image, p - 24), u64_at(&self.image, p - 16));
                if member > 0x10000 || kind > 0x40 || self.file_off(owner).is_none() {
                    continue;
                }
                self.class(owner);
            }
        }
    }

    fn class(&mut self, addr: u64) -> Option<Class> {
        if let Some(c) = self.classes.get(&addr) {
            return Some(c.clone());
        }
        let p = self.file_off(addr).filter(|&p| p + 72 <= self.image.len())?;
        let names = self.file_off(u64_at(&self.image, p))?;
        let name = self.string(u64_at(&self.image, names));
        let size = u64_at(&self.image, p + 48);
        let start = self.file_off(u64_at(&self.image, p + 32))?;
        if !(2..=80).contains(&name.len()) || !(16..=0x10000).contains(&size) {
            return None;
        }
        let mut fields = HashMap::new();
        for i in 0..512 {
            let q = start + i * 40;
            if q + 40 > self.image.len() || u64_at(&self.image, q) != addr {
                break;
            }
            let fname = self.string(u64_at(&self.image, q + 32));
            let (off, kind) = (u64_at(&self.image, q + 8), u64_at(&self.image, q + 16));
            if !fname.is_empty() && off < size && kind <= 0x40 {
                fields.entry(fname).or_insert((off, kind));
            }
        }
        if fields.is_empty() {
            return None;
        }
        self.classes.insert(addr, Class { name, addr, fields, vtable: 0 });
        let parent = u64_at(&self.image, p + 24);
        if let Some(parent) = self.class(parent) {
            let c = self.classes.get_mut(&addr).unwrap();
            for (k, v) in parent.fields {
                c.fields.entry(k).or_insert(v);
            }
        }
        self.classes.get(&addr).cloned()
    }

    /// The vtable (as an offset from the exe's base) of a class: its "get class" function returns
    /// the descriptor, and sits at a known slot of exactly one vtable.
    fn vtable(&self, descriptor: u64, lea: &HashMap<u64, Vec<u64>>) -> u64 {
        for &fun in lea.get(&descriptor).into_iter().flatten() {
            let mut cands: Vec<u64> = find_all(&self.image, &fun.to_le_bytes())
                .into_iter()
                .filter_map(|o| self.va(o))
                .map(|v| v - 40)
                .filter(|&v| v >= self.base && self.code_ptr(v) && self.code_ptr(v + 8))
                .collect();
            cands.sort_unstable();
            cands.dedup();
            if cands.len() == 1 {
                return cands[0] - self.base;
            }
        }
        0
    }
    fn code_ptr(&self, at: u64) -> bool {
        let Some(p) = self.file_off(at).filter(|&p| p + 8 <= self.image.len()) else { return false };
        let v = u64_at(&self.image, p);
        v >= self.base && self.sections.iter().any(|s| s.exec() && v - self.base >= s.rva && v - self.base < s.rva + s.len as u64)
    }

    /// The code that adds engine damage reads the damage settings through a global pointer;
    /// its two operands lead to the active settings. Matched on this build's instructions.
    fn find_damage_root(&self, damage: &Class) -> Option<(u64, u64)> {
        let ratio = damage.f("engine_damage_ratio")?;
        let wear = self.vehicle.f("engine_wear")?;
        const LOAD: [u8; 10] = [0x4c, 0x8b, 0xd1, 0x41, 0x8b, 0x50, 0x48, 0x48, 0x8b, 0x80];
        let mut found = Vec::new();
        for s in self.sections.iter().filter(|s| s.exec()) {
            let code = &self.image[s.off..s.off + s.len];
            for i in 0..code.len().saturating_sub(96) {
                let c = &code[i..i + 96];
                if c[0] != 0x48 || c[1] != 0x8b || c[2] != 0x05 || c[7..17] != LOAD || c[0x47..0x4b] != [0xf3, 0x0f, 0x59, 0x40] || c[0x50..0x55] != [0xf3, 0x41, 0x0f, 0x58, 0x80] {
                    continue;
                }
                if c[0x4b] as u64 != ratio || i32_at(c, 0x55) as u64 != wear {
                    continue;
                }
                let slot = (s.rva as i64 + i as i64 + 7 + i32_at(c, 3) as i64) as u64;
                let member = i32_at(c, 17) as u64;
                if !(16..=65536).contains(&member) || member & 7 != 0 {
                    continue;
                }
                if self.sections.iter().any(|w| w.writable() && slot >= w.rva && slot + 8 <= w.rva + w.vlen as u64) && !found.contains(&(slot, member)) {
                    found.push((slot, member));
                }
            }
        }
        (found.len() == 1).then(|| found[0])
    }
}

/// What the layout found in an exe (for the --ets2-layout dev flag).
pub fn describe(path: &std::path::Path) -> String {
    let started = Instant::now();
    match Layout::read(path) {
        Err(e) => format!("error: {e}"),
        Ok(l) => {
            let mut out = format!("read in {:?}, {} classes
", started.elapsed(), l.classes.len());
            for c in [Some(&l.economy), Some(&l.bank), Some(&l.player), Some(&l.vehicle), l.player_vehicles.as_ref(), l.trailer.as_ref(), l.damage.as_ref()].into_iter().flatten() {
                let pick = ["bank", "player", "experience_points", "money_account", "assigned_truck", "assigned_vehicles", "driving_time", "fuel_relative", "engine_wear", "vehicle", "trailer", "trailer_body_wear", "truck_damage_coef", "engine_damage_ratio"];
                let fields: Vec<String> = pick.iter().filter_map(|f| c.fields.get(*f).map(|(o, k)| format!("{f}@{o:#x}:{k}"))).collect();
                out += &format!("{} vtable {:#x} ({} fields) {}
", c.name, c.vtable, c.fields.len(), fields.join(" "));
            }
            let dmg = l.damage.as_ref().map_or(0, |d| DAMAGE_FIELDS.iter().filter(|f| d.typed(f, FLOAT).is_some()).count());
            out += &format!("damage root {:?}, {dmg}/{} damage fields", l.damage_root, DAMAGE_FIELDS.len());
            out
        }
    }
}

/// Damage settings set to 0 by No Damage (each put back when it's turned off).
const DAMAGE_FIELDS: [&str; 41] = [
    "side_damage_factor", "roof_damage_factor", "truck_damage_coef", "trailer_damage_coef", "dragged_trailer_damage_coef", "truck_to_trailer_dmg", "car_to_cargo_dmg",
    "cabin_damage_ratio", "chassis_damage_ratio", "engine_damage_ratio", "transmission_damage_ratio", "wheel_damage_ratio",
    "cabin_unfixable_damage_ratio", "chassis_unfixable_damage_ratio", "engine_unfixable_damage_ratio", "transmission_unfixable_damage_ratio", "wheel_unfixable_damage_ratio",
    "trailer_body_damage_ratio", "trailer_chassis_damage_ratio", "trailer_wheel_damage_ratio",
    "trailer_body_unfixable_damage_ratio", "trailer_chassis_unfixable_damage_ratio", "trailer_wheel_unfixable_damage_ratio", "cargo_damage_ratio",
    "cabin_wear", "chassis_wear", "engine_wear", "transmission_wear", "wheel_wear",
    "cabin_wear_unfixable", "chassis_wear_unfixable", "engine_wear_unfixable", "transmission_wear_unfixable", "wheel_wear_unfixable",
    "trailer_body_wear", "trailer_chassis_wear", "trailer_wheel_wear",
    "trailer_body_wear_unfixable", "trailer_chassis_wear_unfixable", "trailer_wheel_wear_unfixable", "odometer_wear_unfixable",
];

/// XP needed for each level in the stock game (/def/economy_data_xp.sii); after the last one,
/// each level takes the last amount again.
const LEVEL_XP: [u32; 30] = [
    200, 500, 700, 900, 1000, 1100, 1300, 1600, 1700, 2100, 2300, 2600, 2700, 2900, 3000, 3100, 3400, 3700, 4000, 4300, 4600, 4700, 4900, 5200, 5700, 5900, 6000, 6200, 6600, 6800,
];
/// The XP at which the level after the one `xp` is in starts.
fn next_level_xp(xp: u32) -> u32 {
    let mut start = 0u32;
    for level in 0.. {
        let step = LEVEL_XP[level.min(LEVEL_XP.len() - 1)];
        if start.saturating_add(step) > xp || start == u32::MAX {
            return start.saturating_add(step);
        }
        start += step;
    }
    unreachable!()
}

pub struct Session {
    p: Proc,
    base: u64,
    l: Layout,
    economy: u64,
    economy_slot: u64,
    next_scan: Instant,
    /// No Damage: the damage settings object and its original values (field offset -> value).
    damage: Option<(u64, Vec<(u64, f32)>)>,
    /// Infinite Money: the most the bank has held while it was on.
    money_floor: Option<i64>,
}

impl Session {
    pub fn attach(pid: u32) -> Result<Session, String> {
        let p = Proc::open(pid)?;
        let (base, path) = p.module("eurotrucks2.exe").or_else(|| p.module("amtrucks.exe")).ok_or("The game hasn't started yet")?;
        let started = Instant::now();
        let l = Layout::read(&path)?;
        log(&format!("ets2: layout in {:?}, damage {}", started.elapsed(), if l.damage_root.is_some() { "found" } else { "not found" }));
        Ok(Session { p, base, l, economy: 0, economy_slot: 0, next_scan: Instant::now(), damage: None, money_floor: None })
    }

    fn is(&self, obj: u64, c: &Class) -> bool {
        obj >= 0x10000 && c.vtable != 0 && self.p.read::<u64>(obj) == Some(self.base + c.vtable)
    }
    fn ptr(&self, a: u64) -> u64 {
        self.p.read::<u64>(a).unwrap_or(0)
    }
    fn valid_economy(&self, e: u64) -> bool {
        let l = &self.l;
        if !self.is(e, &l.economy) {
            return false;
        }
        let (Some(bf), Some(pf), Some(mf), Some(xf)) = (l.economy.f("bank"), l.economy.f("player"), l.bank.f("money_account"), l.economy.f("experience_points")) else { return false };
        let bank = self.ptr(e + bf);
        self.is(bank, &l.bank)
            && self.is(self.ptr(e + pf), &l.player)
            && self.p.read::<i64>(bank + mf).is_some_and(|m| (-1_000_000_000..=9_000_000_000_000_000).contains(&m))
            && self.p.read::<i32>(e + xf).is_some_and(|x| x >= 0)
    }
    /// Finds the loaded profile's economy: a pointer to it in the exe's data.
    fn find_economy(&mut self) {
        if self.economy_slot != 0 {
            self.economy = self.ptr(self.economy_slot);
            if self.valid_economy(self.economy) {
                return;
            }
        }
        self.economy = 0;
        if Instant::now() < self.next_scan {
            return;
        }
        self.next_scan = Instant::now() + Duration::from_secs(5);
        let mut hits = Vec::new();
        let mut buf = vec![0u8; 1 << 20];
        let ranges: Vec<(u64, usize)> = self.l.sections.iter().filter(|s| s.writable() && s.vlen <= 64 << 20).map(|s| (s.rva, s.vlen)).collect();
        let mut seen = std::collections::HashSet::new();
        for (rva, len) in ranges {
            let mut off = 0;
            while off < len {
                let n = buf.len().min(len - off);
                let at = self.base + rva + off as u64;
                if self.p.read_into(at, &mut buf[..n]) {
                    for i in (0..n.saturating_sub(7)).step_by(8) {
                        let v = u64_at(&buf, i);
                        if v < 0x1_0000_0000 || v > 0x7fff_ffff_ffff || v & 7 != 0 || (v >= self.base && v < self.base + self.l.size) || !seen.insert(v) {
                            continue;
                        }
                        if self.valid_economy(v) {
                            hits.push((at + i as u64, v));
                        }
                    }
                }
                off += n;
            }
        }
        if hits.len() == 1 {
            (self.economy_slot, self.economy) = hits[0];
        }
    }

    /// The player and their truck and trailer (0 when there's none).
    fn vehicles(&self) -> (u64, u64, u64) {
        let l = &self.l;
        let player = l.economy.f("player").map_or(0, |f| self.ptr(self.economy + f));
        let (mut truck, mut trailer) = (0, 0);
        if let (Some(pv), Some(f)) = (&l.player_vehicles, l.player.f("assigned_vehicles")) {
            let active = self.ptr(player + f);
            if self.is(active, pv) {
                truck = pv.f("vehicle").map_or(0, |f| self.ptr(active + f));
                trailer = pv.f("trailer").map_or(0, |f| self.ptr(active + f));
            }
        }
        if !self.is(truck, &l.vehicle) {
            truck = l.player.f("assigned_truck").map_or(0, |f| self.ptr(player + f));
        }
        if !self.is(truck, &l.vehicle) {
            truck = 0;
        }
        if !l.trailer.as_ref().is_some_and(|t| self.is(trailer, t)) {
            trailer = 0;
        }
        (player, truck, trailer)
    }

    fn unit_float(&self, obj: u64, c: &Class, field: &str) -> Option<u64> {
        let off = c.typed(field, FLOAT)?;
        let v = self.p.read::<f32>(obj + off)?;
        (v.is_finite() && (0.0..=1.001).contains(&v)).then_some(obj + off)
    }
    /// Sets a vehicle's wear to none (the repairable and the permanent part, and the wheels).
    fn repair(&self, obj: u64, c: &Class, fields: &[&str]) {
        for f in fields {
            for name in [f.to_string(), format!("{f}_unfixable")] {
                if let Some(a) = self.unit_float(obj, c, &name) {
                    self.p.write(a, 0f32);
                }
            }
        }
        for name in ["wheels_wear", "wheels_wear_unfixable"] {
            let Some(off) = c.typed(name, FLOAT_ARRAY) else { continue };
            let (data, n) = (self.ptr(obj + off + 8), self.p.read::<u64>(obj + off + 16).unwrap_or(0));
            if data >= 0x10000 && (1..=32).contains(&n) {
                for i in 0..n {
                    self.p.write(data + i * 4, 0f32);
                }
            }
        }
    }

    fn damage_object(&self) -> Option<(u64, &Class)> {
        let (slot, member) = self.l.damage_root?;
        let c = self.l.damage.as_ref()?;
        let obj = self.ptr(self.ptr(self.base + slot) + member);
        self.is(obj, c).then_some((obj, c))
    }
    fn set_no_damage(&mut self, on: bool) {
        match (on, self.damage.take()) {
            (true, Some(d)) => {
                // Still the same settings: keep them at 0 (the game may reload them).
                if self.damage_object().is_some_and(|(o, _)| o == d.0) {
                    for &(off, _) in &d.1 {
                        if self.p.read::<f32>(d.0 + off) != Some(0.0) {
                            self.p.write(d.0 + off, 0f32);
                        }
                    }
                    self.damage = Some(d);
                }
            }
            (true, None) => {
                let Some((obj, c)) = self.damage_object() else { return };
                let mut orig = Vec::new();
                for name in DAMAGE_FIELDS {
                    let Some(off) = c.typed(name, FLOAT) else { continue };
                    let Some(v) = self.p.read::<f32>(obj + off).filter(|v| v.is_finite() && (0.0..=1e6).contains(v)) else { continue };
                    orig.push((off, v));
                    self.p.write(obj + off, 0f32);
                }
                self.damage = Some((obj, orig));
            }
            (false, Some((obj, orig))) => {
                // Put back only what's still ours, on the same object.
                if self.damage_object().is_some_and(|(o, _)| o == obj) {
                    for (off, v) in orig {
                        if self.p.read::<f32>(obj + off) == Some(0.0) {
                            self.p.write(obj + off, v);
                        }
                    }
                }
            }
            (false, None) => {}
        }
    }

    /// Ok(true) once a profile is loaded.
    pub fn tick(&mut self, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Result<bool, String> {
        self.find_economy();
        if self.economy == 0 {
            self.damage = None;
            self.money_floor = None;
            return Ok(false);
        }
        let on = |id: &str| values.and_then(|v| v.get(id)).is_some_and(|&x| x > 0.0);
        let l = &self.l;
        let (bf, mf, xf) = (l.economy.f("bank").unwrap(), l.bank.f("money_account").unwrap(), l.economy.f("experience_points").unwrap());
        let money_at = self.ptr(self.economy + bf) + mf;
        let xp_at = self.economy + xf;
        let (player, truck, trailer) = self.vehicles();

        for a in actions {
            match a.as_str() {
                "add-100-000" => {
                    if let Some(m) = self.p.read::<i64>(money_at) {
                        self.p.write(money_at, m.saturating_add(100_000));
                        self.money_floor = self.money_floor.map(|f| f.max(m + 100_000));
                    }
                }
                "add-10-000-xp" => {
                    if let Some(x) = self.p.read::<u32>(xp_at) {
                        self.p.write(xp_at, x.saturating_add(10_000));
                    }
                }
                "level-up" => {
                    if let Some(x) = self.p.read::<u32>(xp_at) {
                        self.p.write(xp_at, next_level_xp(x));
                    }
                }
                "repair-truck-and-trailer" => {
                    if truck != 0 {
                        self.repair(truck, &self.l.vehicle, &["engine_wear", "transmission_wear", "cabin_wear", "chassis_wear"]);
                    }
                    if let (true, Some(t)) = (trailer != 0, self.l.trailer.as_ref()) {
                        self.repair(trailer, t, &["trailer_body_wear", "chassis_wear"]);
                    }
                }
                "refuel" => {
                    if let Some(a) = self.unit_float(truck, &self.l.vehicle, "fuel_relative") {
                        self.p.write(a, 1f32);
                    }
                }
                _ => {}
            }
        }

        // Infinite Money: the balance never goes down (earnings still raise it).
        if on("infinite-money") {
            if let Some(m) = self.p.read::<i64>(money_at) {
                let floor = self.money_floor.unwrap_or(m).max(m);
                if m < floor {
                    self.p.write(money_at, floor);
                }
                self.money_floor = Some(floor);
            }
        } else {
            self.money_floor = None;
        }
        if on("infinite-fuel") {
            if let Some(a) = self.unit_float(truck, &self.l.vehicle, "fuel_relative") {
                if self.p.read::<f32>(a).is_some_and(|f| f < 0.999) {
                    self.p.write(a, 1f32);
                }
            }
        }
        if on("no-fatigue") {
            if let Some(f) = self.l.player.f("driving_time") {
                self.p.write(player + f, 0i32);
            }
        }
        self.set_no_damage(on("no-damage"));
        Ok(true)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Leaving (game closing or HotSwap stopping): put the damage settings back.
        self.set_no_damage(false);
    }
}
