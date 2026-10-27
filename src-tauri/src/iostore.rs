//! Reads files out of an Unreal Engine 5 IoStore container (.utoc + .ucas), the archive format
//! the game ships in. Used to load the game's own map texture from the user's install, the same
//! way Steam art is read from the local Steam cache; nothing from the game is bundled.

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const HEADER_SIZE: usize = 144;

pub struct Container {
    ucas: PathBuf,
    partition_size: u64,
    block_size: u64,
    /// Per TOC entry: (offset, length) in the uncompressed address space.
    chunks: Vec<(u64, u64)>,
    /// (compressed offset, compressed size, uncompressed size, method index)
    blocks: Vec<(u64, u32, u32, u8)>,
    methods: Vec<String>,
    /// Lowercased full path -> TOC entry index.
    pub files: HashMap<String, usize>,
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

/// Reads an FString: i32 length (negative = UTF-16), including the terminating zero.
fn fstring(d: &[u8], pos: &mut usize) -> Option<String> {
    let len = i32::from_le_bytes(d.get(*pos..*pos + 4)?.try_into().ok()?);
    *pos += 4;
    if len >= 0 {
        let n = len as usize;
        let s = d.get(*pos..*pos + n)?;
        *pos += n;
        Some(String::from_utf8_lossy(s).trim_end_matches('\0').to_string())
    } else {
        let n = (-len) as usize * 2;
        let s = d.get(*pos..*pos + n)?;
        *pos += n;
        let w: Vec<u16> = s.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&w).trim_end_matches('\0').to_string())
    }
}

impl Container {
    pub fn open(utoc: &Path) -> Result<Container, String> {
        let d = std::fs::read(utoc).map_err(|e| e.to_string())?;
        if d.len() < HEADER_SIZE || &d[..16] != b"-==--==--==--==-" {
            return Err("not an IoStore table of contents".into());
        }
        let version = d[16];
        let entries = u32_at(&d, 24) as usize;
        let block_entries = u32_at(&d, 28) as usize;
        let block_entry_size = u32_at(&d, 32) as usize;
        let method_count = u32_at(&d, 36) as usize;
        let method_len = u32_at(&d, 40) as usize;
        let block_size = u32_at(&d, 44) as u64;
        let dir_size = u32_at(&d, 48) as usize;
        let flags = d[80];
        let seeds = u32_at(&d, 84) as usize;
        let partition_size = u64::from_le_bytes(d[88..96].try_into().unwrap());
        let no_hash = u32_at(&d, 96) as usize;
        if flags & 2 != 0 {
            return Err("container is encrypted".into());
        }

        let mut pos = HEADER_SIZE + entries * 12; // chunk IDs
        let chunks = (0..entries)
            .map(|i| {
                let e = &d[pos + i * 10..pos + i * 10 + 10];
                let be = |b: &[u8]| b.iter().fold(0u64, |a, &x| (a << 8) | x as u64);
                (be(&e[..5]), be(&e[5..]))
            })
            .collect();
        pos += entries * 10;
        if version >= 4 {
            pos += seeds * 4;
        }
        if version >= 5 {
            pos += no_hash * 4;
        }
        let blocks = (0..block_entries)
            .map(|i| {
                let e = &d[pos + i * block_entry_size..];
                let le = |b: &[u8]| b.iter().rev().fold(0u64, |a, &x| (a << 8) | x as u64);
                (le(&e[..5]), le(&e[5..8]) as u32, le(&e[8..11]) as u32, e[11])
            })
            .collect();
        pos += block_entries * block_entry_size;
        let mut methods = vec!["None".to_string()];
        for i in 0..method_count {
            let raw = &d[pos + i * method_len..pos + (i + 1) * method_len];
            methods.push(String::from_utf8_lossy(raw).trim_end_matches('\0').to_string());
        }
        pos += method_count * method_len;
        if flags & 4 != 0 {
            // Signed: hash size, TOC and block signatures, then one SHA-1 per block.
            let hash = u32_at(&d, pos) as usize;
            pos += 4 + hash * 2 + block_entries * 20;
        }
        let files = if flags & 8 != 0 { directory(&d[pos..pos + dir_size]).unwrap_or_default() } else { HashMap::new() };
        Ok(Container { ucas: utoc.with_extension("ucas"), partition_size, block_size, chunks, blocks, methods, files })
    }

    /// The uncompressed contents of TOC entry `index`.
    pub fn read(&self, index: usize) -> Result<Vec<u8>, String> {
        let (offset, len) = *self.chunks.get(index).ok_or("no such chunk")?;
        let (first, last) = (offset / self.block_size, (offset + len.max(1) - 1) / self.block_size);
        let mut out = Vec::with_capacity(((last - first + 1) * self.block_size) as usize);
        let mut file = File::open(&self.ucas).map_err(|e| e.to_string())?;
        let mut ooz = oozextract::Extractor::new();
        for b in first..=last {
            let (coff, csize, usize_, method) = *self.blocks.get(b as usize).ok_or("block out of range")?;
            if self.partition_size != 0 && coff >= self.partition_size {
                return Err("multi-partition containers aren't supported".into());
            }
            let mut raw = vec![0u8; csize as usize];
            file.seek(SeekFrom::Start(coff)).and_then(|_| file.read_exact(&mut raw)).map_err(|e| e.to_string())?;
            match self.methods.get(method as usize).map(|s| s.as_str()) {
                _ if method == 0 => out.extend_from_slice(&raw[..usize_ as usize]),
                Some(m) if m.eq_ignore_ascii_case("oodle") => {
                    let mut buf = vec![0u8; usize_ as usize];
                    ooz.read_from_slice(&raw, &mut buf).map_err(|e| format!("oodle: {e:?}"))?;
                    out.extend_from_slice(&buf);
                }
                Some(m) => return Err(format!("unsupported compression {m}")),
                None => return Err("bad compression index".into()),
            }
        }
        let start = (offset - first * self.block_size) as usize;
        Ok(out[start..start + len as usize].to_vec())
    }
}

/// Parses the directory index into full paths.
fn directory(d: &[u8]) -> Option<HashMap<String, usize>> {
    let mut pos = 0;
    let mount = fstring(d, &mut pos)?;
    let count = |pos: &mut usize| -> Option<usize> {
        let n = u32_at(d.get(*pos..*pos + 4)?, 0) as usize;
        *pos += 4;
        Some(n)
    };
    let nd = count(&mut pos)?;
    let dirs: Vec<[u32; 4]> = (0..nd).map(|i| [0, 1, 2, 3].map(|k| u32_at(d, pos + i * 16 + k * 4))).collect();
    pos += nd * 16;
    let nf = count(&mut pos)?;
    let files: Vec<[u32; 3]> = (0..nf).map(|i| [0, 1, 2].map(|k| u32_at(d, pos + i * 12 + k * 4))).collect();
    pos += nf * 12;
    let ns = count(&mut pos)?;
    let mut strings = Vec::with_capacity(ns);
    for _ in 0..ns {
        strings.push(fstring(d, &mut pos)?);
    }
    const NONE: u32 = u32::MAX;
    let name = |i: u32| if i == NONE { String::new() } else { strings.get(i as usize).cloned().unwrap_or_default() };
    let mut out = HashMap::new();
    // Walk the tree from the root directory (entry 0).
    let mut stack = vec![(0u32, mount.trim_end_matches('/').to_string())];
    let mut guard = 0;
    while let Some((di, path)) = stack.pop() {
        guard += 1;
        if guard > 2_000_000 {
            break;
        }
        let Some(dir) = dirs.get(di as usize) else { continue };
        let mut f = dir[3];
        while f != NONE {
            let Some(fe) = files.get(f as usize) else { break };
            out.insert(format!("{path}/{}", name(fe[0])).to_ascii_lowercase(), fe[2] as usize);
            f = fe[1];
        }
        let mut c = dir[1];
        while c != NONE {
            let Some(ce) = dirs.get(c as usize) else { break };
            stack.push((c, format!("{path}/{}", name(ce[0]))));
            c = ce[2];
        }
    }
    Some(out)
}
