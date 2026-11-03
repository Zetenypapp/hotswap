//! Finds a UE5 game's object table and name pool without debug symbols, by looking for their
//! distinctive layouts in the game module's data sections. Every candidate is checked before use.

use super::mem::Proc;

/// (name, address, size) of each section of the module loaded at `base`.
pub fn sections(p: &Proc, base: u64) -> Vec<(String, u64, u64)> {
    sections_with_flags(p, base).into_iter().map(|(n, a, s, _)| (n, a, s)).collect()
}

/// Where the tables can be: writable sections. Usually ".data", but protected games (Denuvo)
/// rename and reshuffle their sections; huge ones (protection code) are skipped.
fn data_sections(p: &Proc, base: u64) -> Vec<(u64, u64)> {
    const WRITE: u32 = 0x8000_0000;
    sections_with_flags(p, base).into_iter().filter(|(_, _, size, flags)| flags & WRITE != 0 && *size > 0 && *size < 128 << 20).map(|(_, a, s, _)| (a, s)).collect()
}

/// (name, address, size, characteristics) of each section.
fn sections_with_flags(p: &Proc, base: u64) -> Vec<(String, u64, u64, u32)> {
    let Some(pe) = p.read::<u32>(base + 0x3C) else { return vec![] };
    let pe = base + pe as u64;
    let (Some(n), Some(opt)) = (p.read::<u16>(pe + 6), p.read::<u16>(pe + 20)) else { return vec![] };
    let first = pe + 24 + opt as u64;
    (0..n as u64)
        .filter_map(|i| {
            let s = first + i * 40;
            let mut name = [0u8; 8];
            p.read_into(s, &mut name).then_some(())?;
            let size = p.read::<u32>(s + 8)? as u64;
            let va = p.read::<u32>(s + 12)? as u64;
            let flags = p.read::<u32>(s + 36)?;
            Some((String::from_utf8_lossy(&name).trim_end_matches('\0').to_string(), base + va, size, flags))
        })
        .collect()
}

/// Reads a section in pieces (parts that can't be read stay zero).
fn read_section(p: &Proc, addr: u64, size: u64) -> Vec<u8> {
    let mut buf = vec![0u8; size as usize];
    const STEP: usize = 1 << 20;
    for (i, chunk) in buf.chunks_mut(STEP).enumerate() {
        p.read_into(addr + (i * STEP) as u64, chunk);
    }
    buf
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn heap_ptr(v: u64) -> bool {
    v > 0x10000 && v < 0x7FFF_FFFF_FFFF && v % 8 == 0
}

/// Candidates for UE 4.22 and older, whose names are in TNameEntryArray: GNames is a pointer in the
/// module's data to a table of chunk pointers. Returns the table addresses (checked by the caller).
pub fn find_name_arrays(p: &Proc, base: u64) -> Vec<u64> {
    let mut out = Vec::new();
    for (addr, size) in data_sections(p, base) {
        let d = read_section(p, addr, size);
        for o in (0..d.len().saturating_sub(8)).step_by(8) {
            let v = u64_at(&d, o);
            // The table's first chunk holds entry pointers; entry 0 is "None" at +0xC.
            if heap_ptr(v) && !out.contains(&v) {
                let entry0 = p.ptr(v).and_then(|chunk| p.ptr(chunk));
                let mut name = [0u8; 5];
                if entry0.is_some_and(|e| heap_ptr(e) && p.read_into(e + 0xC, &mut name)) && &name == b"None\0" {
                    out.push(v);
                }
            }
        }
    }
    out
}

/// Candidate addresses of GUObjectArray (FUObjectArray) and of the FName pool, best first.
pub fn find_tables(p: &Proc, base: u64) -> (Vec<u64>, Vec<u64>) {
    let (mut objects, mut names) = (Vec::new(), Vec::new());
    for (addr, size) in data_sections(p, base) {
        let d = read_section(p, addr, size);
        let mut o = 0;
        while o + 0x40 <= d.len() {
            // FUObjectArray: 4 small header fields, then the chunked array at +0x10:
            // Objects*, PreAllocatedObjects*, MaxElements, NumElements, MaxChunks, NumChunks.
            let (max, num, max_chunks, chunks) = (u32_at(&d, o + 0x20), u32_at(&d, o + 0x24), u32_at(&d, o + 0x28), u32_at(&d, o + 0x2C));
            if heap_ptr(u64_at(&d, o + 0x10))
                && (10_000..20_000_000).contains(&num)
                && max >= num
                && max_chunks == max.div_ceil(65536)
                && chunks == num.div_ceil(65536)
            {
                objects.push(addr + o as u64);
            }
            // FNamePool: lock, CurrentBlock, CurrentByteCursor, then Blocks[]: the first
            // CurrentBlock + 1 are set and the next one is empty.
            let (cur, cursor) = (u32_at(&d, o + 8) as usize, u32_at(&d, o + 12));
            if cur < 8192 && cursor > 0 && cursor < 0x40000 && o + 0x10 + (cur + 2) * 8 <= d.len() {
                let blocks = o + 0x10;
                if (0..=cur).all(|i| heap_ptr(u64_at(&d, blocks + i * 8))) && u64_at(&d, blocks + (cur + 1) * 8) == 0 {
                    names.push(addr + o as u64);
                }
            }
            o += 8;
        }
    }
    (objects, names)
}
