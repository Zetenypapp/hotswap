//! Reads Steam's binary `appcache/appinfo.vdf` to get each app's `common/type`
//! ("Game", "Tool", "Application", ...). Supports format v28 and v29.

use std::collections::{HashMap, HashSet};
use std::{fs, path::Path};

const V28: u32 = 0x0756_4428;
const V29: u32 = 0x0756_4429;
/// info_state, last_updated, pics_token, text sha1, change_number, binary sha1
const ENTRY_HEADER: usize = 4 + 4 + 8 + 20 + 4 + 20;

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> Option<u32> {
        let b = self.buf.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes(b.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        let b = self.buf.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        Some(u64::from_le_bytes(b.try_into().ok()?))
    }
    fn byte(&mut self) -> Option<u8> {
        let b = *self.buf.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }
    fn cstr(&mut self) -> Option<&'a str> {
        let rest = self.buf.get(self.pos..)?;
        let len = rest.iter().position(|&b| b == 0)?;
        self.pos += len + 1;
        std::str::from_utf8(&rest[..len]).ok()
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        (self.pos + n <= self.buf.len()).then(|| self.pos += n)
    }
}

/// Flattened fields per app, keyed like `common/type` or `common/category/category_8`
/// (the leading `appinfo/` is dropped). Only string and integer values are kept.
pub type Fields = HashMap<String, String>;

pub fn app_fields(steam_root: &Path, wanted: &HashSet<u32>) -> HashMap<u32, Fields> {
    let mut out = HashMap::new();
    let Ok(buf) = fs::read(steam_root.join("appcache").join("appinfo.vdf")) else { return out };
    let mut r = Reader { buf: &buf, pos: 0 };
    let Some(magic) = r.u32() else { return out };
    if r.u32().is_none() {
        return out;
    }

    // v29 stores key names once in a string table at the end of the file.
    let strings: Option<Vec<&str>> = match magic {
        V29 => {
            let Some(offset) = r.u64() else { return out };
            let mut t = Reader { buf: &buf, pos: offset as usize };
            let Some(count) = t.u32() else { return out };
            let mut list = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let Some(s) = t.cstr() else { return out };
                list.push(s);
            }
            Some(list)
        }
        V28 => None,
        _ => return out,
    };

    while let Some(app_id) = r.u32() {
        if app_id == 0 {
            break;
        }
        let Some(size) = r.u32() else { break };
        let end = r.pos + size as usize;
        if end > buf.len() {
            break;
        }
        if wanted.contains(&app_id) {
            let mut e = Reader { buf: &buf[..end], pos: r.pos + ENTRY_HEADER };
            out.insert(app_id, flatten(&mut e, strings.as_deref()));
        }
        r.pos = end;
    }
    out
}

/// Walks one binary KeyValues blob and flattens it. Stops quietly on malformed data.
fn flatten(r: &mut Reader<'_>, strings: Option<&[&str]>) -> Fields {
    let mut out = Fields::new();
    let _ = walk(r, strings, &mut out);
    out
}

fn walk(r: &mut Reader<'_>, strings: Option<&[&str]>, out: &mut Fields) -> Option<()> {
    let mut path: Vec<String> = Vec::new();
    let join = |path: &[String], key: &str| {
        let mut parts: Vec<&str> = path.iter().skip(1).map(String::as_str).collect();
        parts.push(key);
        parts.join("/").to_ascii_lowercase()
    };
    loop {
        let kind = r.byte()?;
        if kind == 0x08 || kind == 0x0B {
            path.pop()?;
            if path.is_empty() {
                return Some(());
            }
            continue;
        }
        let key = match strings {
            Some(table) => *table.get(r.u32()? as usize)?,
            None => r.cstr()?,
        };
        match kind {
            0x00 => path.push(key.to_string()),
            0x01 => {
                let value = r.cstr()?;
                out.insert(join(&path, key), value.to_string());
            }
            0x02 => {
                let v = r.u32()? as i32;
                out.insert(join(&path, key), v.to_string());
            }
            0x03 | 0x04 | 0x06 => r.skip(4)?,
            0x07 | 0x0A => {
                let v = r.u64()?;
                out.insert(join(&path, key), v.to_string());
            }
            0x05 => {
                // UTF-16 string: runs until a 0x0000 code unit
                while r.u16_is_nonzero()? {}
            }
            _ => return None,
        }
    }
}

impl Reader<'_> {
    fn u16_is_nonzero(&mut self) -> Option<bool> {
        let b = self.buf.get(self.pos..self.pos + 2)?;
        self.pos += 2;
        Some(b != [0, 0])
    }
}
