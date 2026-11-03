//! Farming Simulator 19 (Steam app 787860) and 22 (1248130). No memory reading: HotSwap puts a small
//! mod (assets/fs_hotswap.lua) in the game's mods folder, and the game runs it in saves where it's
//! turned on. The game's Lua can't reach the network, so the two talk through files in the game's
//! modSettings/HotSwap folder: in.xml (which mods are on, written here) and out.xml (written by the
//! mod once a second, with whether this game is the server). Only the server (single-player or the
//! host) changes anything, so it works in multiplayer games you host but not ones you join.

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    time::{Duration, SystemTime},
};

pub const FS19: &str = "787860";
pub const FS22: &str = "1248130";
const LUA: &str = include_str!("../assets/fs_hotswap.lua");

/// The game's own folder under Documents\My Games (Windows may have moved Documents, e.g. to
/// OneDrive, so it comes from the registry).
fn game_dir(game: &str) -> Option<PathBuf> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let docs: String = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
        .and_then(|k| k.get_value("Personal"))
        .ok()
        .or_else(|| Some(format!(r"{}\Documents", std::env::var("USERPROFILE").ok()?)))?;
    let year = if game == FS19 { "2019" } else { "2022" };
    Some(PathBuf::from(docs).join("My Games").join(format!("FarmingSimulator{year}")))
}

fn mod_desc(game: &str) -> String {
    // The newest mod format each game reads (as big mods like Courseplay use).
    let version = if game == FS19 { 53 } else { 80 };
    format!(
        r#"<?xml version="1.0" encoding="utf-8" standalone="no" ?>
<modDesc descVersion="{version}">
    <author>HotSwap</author>
    <version>1.0.0.0</version>
    <title><en>HotSwap</en></title>
    <description><en><![CDATA[Lets the HotSwap app change your money and fuel while you play. Works in single-player and in multiplayer games you host; it does nothing in games you join.]]></en></description>
    <iconFilename>icon_hotswap.dds</iconFilename>
    <multiplayer supported="true"/>
    <extraSourceFiles>
        <sourceFile filename="hotswap.lua"/>
    </extraSourceFiles>
</modDesc>
"#
    )
}

/// The mod's icon in the mod list: HotSwap's logo (the two arrows on green), drawn here as an
/// uncompressed 256x256 DDS, the format the game reads.
fn icon() -> Vec<u8> {
    const N: usize = 256;
    let mut out = Vec::with_capacity(128 + N * N * 4);
    let mut dword = |v: u32| out.extend_from_slice(&v.to_le_bytes());
    // "DDS ", header size, flags (caps, height, width, pitch, pixel format), height, width, pitch.
    for v in [0x2053_4444, 124, 0x100F, N as u32, N as u32, (N * 4) as u32, 0, 0] {
        dword(v);
    }
    for _ in 0..11 {
        dword(0);
    }
    // Pixel format: 32-bit BGRA. Then caps: a texture.
    for v in [32, 0x41, 0, 32, 0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000, 0x1000, 0, 0, 0, 0] {
        dword(v);
    }
    // The logo's strokes, in its 24-unit drawing (docs/banner.html).
    let lines = [((5.5, 9.5), (17.0, 9.5)), ((17.0, 9.5), (14.0, 6.9)), ((18.5, 14.5), (7.0, 14.5)), ((7.0, 14.5), (10.0, 17.1))];
    let dist = |(px, py): (f32, f32), ((ax, ay), (bx, by)): ((f32, f32), (f32, f32))| {
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        ((px - ax - t * dx).powi(2) + (py - ay - t * dy).powi(2)).sqrt()
    };
    for y in 0..N {
        for x in 0..N {
            let p = ((x as f32 + 0.5) * 24.0 / N as f32, (y as f32 + 0.5) * 24.0 / N as f32);
            let on = lines.iter().any(|&l| dist(p, l) < 1.05);
            // BGRA: dark ink on HotSwap green.
            out.extend_from_slice(if on { &[0x0E, 0x14, 0x06, 0xFF] } else { &[0x9A, 0xD4, 0x3F, 0xFF] });
        }
    }
    out
}

/// Puts the mod (a zip: unzipped mods can't be used in multiplayer) in the game's mods folder, or
/// updates it. Only when the game has made its folder, so nothing is created for people who never
/// started it.
pub fn install(game: &str) -> Result<PathBuf, String> {
    let dir = game_dir(game).ok_or("No Documents folder")?;
    if !dir.is_dir() {
        return Err("Start Farming Simulator once first".into());
    }
    let mods = dir.join("mods");
    fs::create_dir_all(&mods).map_err(|e| e.to_string())?;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in [("modDesc.xml", mod_desc(game).into_bytes()), ("hotswap.lua", LUA.as_bytes().to_vec()), ("icon_hotswap.dds", icon())] {
            z.start_file(name, opts).map_err(|e| e.to_string())?;
            z.write_all(&data).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    let path = mods.join("HotSwap.zip");
    // Rewritten only when the mod changed: the zip's bytes are the same for the same files. A running
    // game may hold the old one open; that one still works, and the next start updates it.
    if fs::read(&path).ok().as_deref() != Some(buf.get_ref().as_slice()) {
        if let Err(e) = fs::write(&path, buf.get_ref()) {
            if !path.exists() {
                return Err(e.to_string());
            }
        }
    }
    Ok(dir)
}

pub struct Session {
    dir: PathBuf,
    run: u32,
    last: String,
    /// Times each action was pressed this game run.
    actions: HashMap<String, u64>,
}

/// What the mod last said: whether it runs (a save with it is open) and whether it's the server.
pub enum Seen {
    No,
    Host,
    Joined,
}

impl Session {
    pub fn attach(game: &str) -> Result<Session, String> {
        let dir = install(game)?.join(r"modSettings\HotSwap");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let run = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.subsec_nanos() | 1).unwrap_or(1);
        Ok(Session { dir, run, last: String::new(), actions: HashMap::new() })
    }

    pub fn tick(&mut self, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Seen {
        for a in actions {
            *self.actions.entry(a.clone()).or_default() += 1;
        }
        let mut xml = format!("<hs run=\"{}\">", self.run);
        let mut on: Vec<_> = values.into_iter().flatten().collect();
        on.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in on {
            xml += &format!("<m k=\"{k}\" v=\"{v}\"/>");
        }
        for (k, n) in &self.actions {
            xml += &format!("<a k=\"{k}\" n=\"{n}\"/>");
        }
        xml += "</hs>\n";
        if xml != self.last {
            // Written whole, then renamed, so the game never reads half a file.
            let tmp = self.dir.join("in.tmp");
            if fs::write(&tmp, &xml).is_ok() && fs::rename(&tmp, self.dir.join("in.xml")).is_ok() {
                self.last = xml;
            }
        }
        let out = self.dir.join("out.xml");
        let fresh = fs::metadata(&out).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d < Duration::from_secs(4));
        if !fresh {
            return Seen::No;
        }
        if fs::read_to_string(&out).unwrap_or_default().contains("server=\"true\"") { Seen::Host } else { Seen::Joined }
    }
}
