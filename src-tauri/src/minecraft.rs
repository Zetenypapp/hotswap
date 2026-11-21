//! Minecraft (Java Edition) on the Maps page, through HotSwap Link: a small client-side Fabric mod
//! (minecraft-mod/ in this repo). The mod listens on 127.0.0.1 and writes its port and a
//! per-session token to ~/.hotswap/minecraft-link.json. It shares only your own position and
//! the colours of the chunks your game has loaded, plus, in single-player, a terrain preview from
//! the world's own generator. Nothing is sent to the game: the map only reads.
//!
//! The map is kept per world and dimension in the app data folder (map/minecraft/<world>/<dim>/):
//! r.X.Z.png explored regions (512 x 512 blocks, 1 pixel per block, transparent where unseen) and
//! g.X.Z.png seed previews (128 x 128, 1 pixel per 4 x 4 blocks).

use base64::Engine as _;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Manager, State};

const REGION: i32 = 512;
const PREVIEW: usize = 128;
const CACHED_REGIONS: usize = 24;

struct Link {
    port: u16,
    token: String,
}

fn link() -> Option<Link> {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    let v: Value = serde_json::from_str(&std::fs::read_to_string(home.join(".hotswap").join("minecraft-link.json")).ok()?).ok()?;
    if v["protocol"].as_u64() != Some(1) {
        return None;
    }
    let port = u16::try_from(v["port"].as_u64()?).ok().filter(|&p| p > 0)?;
    let token = v["token"].as_str()?.to_string();
    if token.is_empty() || token.len() > 128 || !token.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    Some(Link { port, token })
}

/// One GET to the mod. Answers can be a few hundred KB (chunk colours).
fn get(l: &Link, path: &str, timeout: Duration) -> Result<(u16, Value), String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], l.port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(400)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(timeout)).ok();
    s.set_write_timeout(Some(Duration::from_millis(1500))).ok();
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        l.port, l.token
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| e.to_string())?;
    let code: u16 = line.split(' ').nth(1).and_then(|c| c.parse().ok()).ok_or("Bad answer from Minecraft")?;
    let mut len = None;
    loop {
        line.clear();
        if r.read_line(&mut line).map_err(|e| e.to_string())? == 0 || line == "\r\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse::<usize>().ok();
            }
        }
    }
    let len = len.ok_or("Bad answer from Minecraft")?;
    if len > 16 << 20 {
        return Err("Answer too big".into());
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok((code, serde_json::from_slice(&body).unwrap_or(Value::Null)))
}

/// Folder-safe form of a world id or dimension ("mp:play.example.net" -> "mp_play.example.net").
fn safe(s: &str) -> String {
    let t: String = s
        .chars()
        .take(80)
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    if t.is_empty() || t.chars().all(|c| c == '.') {
        "_".into()
    } else {
        t
    }
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("map").join("minecraft"))
}

fn dim_dir(app: &AppHandle, world: &str, dim: &str) -> Result<PathBuf, String> {
    Ok(root(app)?.join(safe(world)).join(safe(dim)))
}

#[derive(Default)]
struct Sync {
    /// Which mod session and world `since` belongs to.
    session: String,
    world: String,
    since: u64,
    /// Decoded explored regions (RGBA), most recently used last.
    regions: Vec<(PathBuf, Vec<u8>)>,
    /// Regions with pixels not saved yet.
    dirty: Vec<PathBuf>,
}

#[derive(Default)]
pub struct Minecraft(Arc<Mutex<Sync>>);

fn read_png(path: &Path) -> Option<Vec<u8>> {
    let dec = png::Decoder::new(BufReader::new(std::fs::File::open(path).ok()?));
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    (info.width == REGION as u32 && info.height == REGION as u32 && info.color_type == png::ColorType::Rgba).then(|| {
        buf.truncate(info.buffer_size());
        buf
    })
}

fn write_png(path: &Path, data: &[u8], size: u32, color: png::ColorType) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    {
        let f = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
        let mut enc = png::Encoder::new(f, size, size);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        enc.write_header().and_then(|mut w| w.write_image_data(data)).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

impl Sync {
    fn region(&mut self, path: &Path) -> &mut Vec<u8> {
        if let Some(i) = self.regions.iter().position(|r| r.0 == path) {
            let r = self.regions.remove(i);
            self.regions.push(r);
        } else {
            let data = read_png(path).unwrap_or_else(|| vec![0; (REGION * REGION * 4) as usize]);
            self.regions.push((path.to_path_buf(), data));
            if self.regions.len() > CACHED_REGIONS {
                // Save before forgetting it, so a sync touching many regions loses nothing.
                let (old, data) = self.regions.remove(0);
                if let Some(i) = self.dirty.iter().position(|d| *d == old) {
                    self.dirty.remove(i);
                    let _ = std::fs::create_dir_all(old.parent().unwrap());
                    let _ = write_png(&old, &data, REGION as u32, png::ColorType::Rgba);
                }
            }
        }
        &mut self.regions.last_mut().unwrap().1
    }
}

/// The mod's status, as the Maps page shows it. `state` is one of:
/// - "nomod": HotSwap Link isn't installed (or Minecraft never ran with it)
/// - "closed": Minecraft isn't running
/// - "menu": running, not in a world
/// - "blocked": in a world on a server that turned HotSwap's map off
/// - "ingame": then world, worldName, dim, x, y, z, yaw and canGenerate
fn status(l: Option<&Link>) -> Value {
    let Some(l) = l else { return json!({ "state": "nomod" }) };
    match get(l, "/status", Duration::from_millis(1500)) {
        Ok((200, v)) => shape(&v),
        _ => json!({ "state": "closed" }),
    }
}

fn text(x: &Value, n: usize) -> String {
    x.as_str().unwrap_or("").chars().take(n).collect()
}

/// Mobs, animals, villagers, dropped items and players, with only the fields the map uses.
fn entities(v: &Value) -> Value {
    let kinds = ["player", "hostile", "villager", "passive", "item", "other"];
    json!(v.as_array().into_iter().flatten().take(400).filter_map(|e| {
        let kind = e["k"].as_str().filter(|k| kinds.contains(k))?;
        Some(json!({ "i": e["i"].as_i64(), "k": kind, "n": text(&e["n"], 48), "x": e["x"].as_f64()?, "y": e["y"].as_f64().unwrap_or(0.0), "z": e["z"].as_f64()? }))
    }).collect::<Vec<_>>())
}

/// The mod's /status answer (or a full status line of the stream), as the map gets it.
fn shape(v: &Value) -> Value {
    match () {
        _ if v["inWorld"].as_bool() == Some(true) => {
            let mut st = json!({
                "state": "ingame",
                "world": v["world"].as_str().unwrap_or(""),
                "worldName": text(&v["worldName"], 80),
                "dim": v["dim"].as_str().unwrap_or("minecraft:overworld"),
                "x": v["x"].as_f64().unwrap_or(0.0),
                "y": v["y"].as_f64().unwrap_or(0.0),
                "z": v["z"].as_f64().unwrap_or(0.0),
                "yaw": v["yaw"].as_f64().unwrap_or(0.0),
                "canGenerate": v["canGenerate"].as_bool().unwrap_or(false),
            });
            // The slower reads: your stats, the clock, the weather, your biome, where you died.
            for k in ["health", "maxHealth", "food", "armor", "xp", "day", "time"] {
                if let Some(n) = v[k].as_f64() {
                    st[k] = json!(n);
                }
            }
            for k in ["raining", "thundering"] {
                if let Some(b) = v[k].as_bool() {
                    st[k] = json!(b);
                }
            }
            st["biome"] = json!(text(&v["biome"], 48));
            if v["death"].is_object() {
                st["death"] = json!({
                    "dim": text(&v["death"]["dim"], 128),
                    "x": v["death"]["x"].as_f64().unwrap_or(0.0),
                    "y": v["death"]["y"].as_f64().unwrap_or(0.0),
                    "z": v["death"]["z"].as_f64().unwrap_or(0.0),
                });
            }
            // What the server allows the map to show (absent: everything, single-player).
            if v["allowed"].is_object() {
                let mut allowed = json!({});
                for k in ["hostile", "passive", "villager", "player", "item", "other"] {
                    allowed[k] = json!(v["allowed"][k].as_bool().unwrap_or(false));
                }
                st["allowed"] = allowed;
            }
            // Mobs, animals, villagers, dropped items and players the game has loaded around you.
            st["entities"] = entities(&v["entities"]);
            // Everyone online, when the server shares that.
            if v["players"].is_array() {
                st["players"] = json!(v["players"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(64)
                    .filter_map(|p| Some(json!({
                        "name": text(&p["name"], 48),
                        "dim": text(&p["dim"], 128),
                        "x": p["x"].as_f64()?, "y": p["y"].as_f64().unwrap_or(0.0), "z": p["z"].as_f64()?,
                        "yaw": p["yaw"].as_f64().unwrap_or(0.0),
                    })))
                    .collect::<Vec<_>>());
            }
            st
        }
        _ if v["blocked"].as_bool() == Some(true) => json!({ "state": "blocked" }),
        _ => json!({ "state": "menu" }),
    }
}

/// The live feed: HotSwap Link 1.3+ streams one line per game tick. While the Minecraft map is
/// open, this reads it on its own thread and passes each line to the map as an `mc-live` event:
/// {t:"s", ...full status} twice a second, {t:"p", dim, x, y, z, yaw, entities?} every tick.
/// An older mod has no stream: then {t:"old"} once, and the map keeps polling.
#[derive(Default)]
pub struct Live(Arc<std::sync::atomic::AtomicU64>);

#[tauri::command]
pub fn mc_live(app: AppHandle, live: State<'_, Live>, on: bool) {
    use std::sync::atomic::Ordering;
    let gen = live.0.fetch_add(1, Ordering::SeqCst) + 1;
    if !on {
        return;
    }
    let current = live.0.clone();
    std::thread::spawn(move || {
        use tauri::Emitter;
        let alive = || current.load(Ordering::SeqCst) == gen;
        while alive() {
            match stream(&app, &alive) {
                // An older mod: say so once and stop; the map polls instead.
                Ok(false) => {
                    let _ = app.emit_to("main", "mc-live", json!({ "t": "old" }));
                    return;
                }
                // Game closed, left the world or restarted: tell the map, then try again soon.
                _ => {
                    if alive() {
                        let _ = app.emit_to(
                            "main",
                            "mc-live",
                            json!({ "t": "s", "state": if link().is_some() { "closed" } else { "nomod" } }),
                        );
                    }
                    for _ in 0..10 {
                        if !alive() {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        }
    });
}

/// One connection to the mod's stream, read until it ends. Ok(false): the mod has no stream.
fn stream(app: &AppHandle, alive: &dyn Fn() -> bool) -> Result<bool, String> {
    let l = link().ok_or("no mod")?;
    let addr = SocketAddr::from(([127, 0, 0, 1], l.port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(400)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(3))).ok();
    s.set_nodelay(true).ok();
    let req = format!(
        "GET /stream HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        l.port, l.token
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).map_err(|e| e.to_string())?;
    let code: u16 = line.split(' ').nth(1).and_then(|c| c.parse().ok()).ok_or("bad answer")?;
    if code == 404 {
        return Ok(false);
    }
    if code != 200 {
        return Err(format!("stream answered {code}"));
    }
    let mut chunked = false;
    loop {
        line.clear();
        if r.read_line(&mut line).map_err(|e| e.to_string())? == 0 || line == "\r\n" {
            break;
        }
        if line.to_ascii_lowercase().starts_with("transfer-encoding:") && line.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        }
    }
    // The body is chunked HTTP: decode the chunks into bytes, then split those into JSON lines.
    let mut buf: Vec<u8> = Vec::new();
    while alive() {
        if chunked {
            line.clear();
            if r.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
                return Err("stream ended".into());
            }
            let size = usize::from_str_radix(line.trim().split(';').next().unwrap_or(""), 16).map_err(|_| "bad chunk")?;
            if size == 0 {
                return Err("stream ended".into());
            }
            if size > 4 << 20 {
                return Err("chunk too big".into());
            }
            let at = buf.len();
            buf.resize(at + size, 0);
            r.read_exact(&mut buf[at..]).map_err(|e| e.to_string())?;
            let mut crlf = [0u8; 2];
            r.read_exact(&mut crlf).map_err(|e| e.to_string())?;
        } else {
            let mut tmp = [0u8; 8192];
            let n = r.read(&mut tmp).map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("stream ended".into());
            }
            buf.extend_from_slice(&tmp[..n]);
        }
        if buf.len() > 4 << 20 {
            return Err("line too long".into());
        }
        while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = buf.drain(..=nl).collect();
            let Ok(v) = serde_json::from_slice::<Value>(&raw) else { continue };
            emit_line(app, &v);
        }
    }
    Ok(true)
}

fn emit_line(app: &AppHandle, v: &Value) {
    use tauri::Emitter;
    let out = match v["t"].as_str() {
        Some("p") => {
            let mut o = json!({
                "t": "p",
                "dim": text(&v["dim"], 128),
                "x": v["x"].as_f64().unwrap_or(0.0),
                "y": v["y"].as_f64().unwrap_or(0.0),
                "z": v["z"].as_f64().unwrap_or(0.0),
                "yaw": v["yaw"].as_f64().unwrap_or(0.0),
            });
            // 10 times a second: compact [id, kind, x, z], since low memory mode gives the
            // page only a small JavaScript heap. Names come with the full status twice a second.
            if v["entities"].is_array() {
                o["entities"] = json!(entities(&v["entities"])
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|e| json!([
                        e["i"],
                        e["k"],
                        (e["x"].as_f64().unwrap_or(0.0) * 10.0).round() / 10.0,
                        (e["z"].as_f64().unwrap_or(0.0) * 10.0).round() / 10.0
                    ]))
                    .collect::<Vec<_>>());
            }
            o
        }
        _ => {
            let mut o = shape(&v);
            o["t"] = json!("s");
            o
        }
    };
    let _ = app.emit_to("main", "mc-live", out);
}

/// Just the status: your position, stats and what's around you. Cheap, polled several times a
/// second so the map moves smoothly; chunks come separately in mc_sync.
#[tauri::command]
pub async fn mc_status() -> Value {
    tauri::async_runtime::spawn_blocking(|| status(link().as_ref()))
        .await
        .unwrap_or_else(|_| json!({ "state": "closed" }))
}

/// Reads the mod's status and saves any new chunk colours into the map. Called about once a
/// second while the Minecraft map is open. Returns the status plus `changed`: the explored
/// regions [rx, rz] of the current dimension that got new pixels.
#[tauri::command]
pub async fn mc_sync(app: AppHandle, mc: State<'_, Minecraft>) -> Result<Value, String> {
    let l = link();
    let mut st = tauri::async_runtime::spawn_blocking({
        let l = l.as_ref().map(|l| Link {
            port: l.port,
            token: l.token.clone(),
        });
        move || status(l.as_ref())
    })
    .await
    .map_err(|e| e.to_string())?;
    if st["state"] != "ingame" {
        return Ok(st);
    }
    let l = l.unwrap();
    let world = st["world"].as_str().unwrap_or("").to_string();
    let dim = st["dim"].as_str().unwrap_or("").to_string();
    let root = root(&app)?;
    // Chunks come in batches; save them region by region.
    let changed = tauri::async_runtime::spawn_blocking({
        let (shared, world, dim, root) = (mc.0.clone(), world.clone(), dim.clone(), root.clone());
        move || pull(&shared, &l, &world, &dim, &root)
    })
    .await
    .map_err(|e| e.to_string())??;
    let wdir = root.join(safe(&world));
    let _ = std::fs::create_dir_all(&wdir);
    let _ = std::fs::write(wdir.join("name.txt"), st["worldName"].as_str().unwrap_or(""));
    st["changed"] = json!(changed);
    Ok(st)
}

fn pull(shared: &Mutex<Sync>, l: &Link, world: &str, dim: &str, root: &Path) -> Result<Vec<[i32; 2]>, String> {
    let mut s = shared.lock().map_err(|_| "busy")?;
    let session = format!("{}:{}", l.port, l.token);
    if s.session != session || s.world != world {
        s.session = session;
        s.world = world.to_string();
        s.since = 0;
    }
    let mut changed: Vec<[i32; 2]> = vec![];
    let started = std::time::Instant::now();
    for _ in 0..6 {
        let (code, v) = get(l, &format!("/chunks?since={}", s.since), Duration::from_secs(3))?;
        if code != 200 || v["world"].as_str() != Some(world) {
            break;
        }
        for c in v["chunks"].as_array().into_iter().flatten() {
            let (Some(d), Some(cx), Some(cz), Some(b64)) = (c["d"].as_str(), c["x"].as_i64(), c["z"].as_i64(), c["c"].as_str()) else {
                continue;
            };
            let Ok(rgb) = base64::engine::general_purpose::STANDARD.decode(b64) else {
                continue;
            };
            if rgb.len() != 16 * 16 * 3 || cx.abs() > 1 << 22 || cz.abs() > 1 << 22 {
                continue;
            }
            let (cx, cz) = (cx as i32, cz as i32);
            let (rx, rz) = (cx.div_euclid(32), cz.div_euclid(32));
            let dir = root.join(safe(world)).join(safe(d));
            let path = dir.join(format!("r.{rx}.{rz}.png"));
            let px = s.region(&path);
            let (ox, oz) = (cx.rem_euclid(32) as usize * 16, cz.rem_euclid(32) as usize * 16);
            let mut new = false;
            for z in 0..16 {
                for x in 0..16 {
                    let i = ((oz + z) * REGION as usize + ox + x) * 4;
                    let j = (z * 16 + x) * 3;
                    if px[i..i + 3] != rgb[j..j + 3] || px[i + 3] != 255 {
                        px[i..i + 3].copy_from_slice(&rgb[j..j + 3]);
                        px[i + 3] = 255;
                        new = true;
                    }
                }
            }
            if !new {
                continue;
            }
            if !s.dirty.contains(&path) {
                s.dirty.push(path);
            }
            if d == dim && !changed.contains(&[rx, rz]) {
                changed.push([rx, rz]);
            }
        }
        s.since = v["rev"].as_u64().unwrap_or(s.since);
        // A big shared map arrives over several syncs instead of holding this one up.
        if v["more"].as_bool() != Some(true) || started.elapsed() > Duration::from_millis(400) {
            break;
        }
    }
    // Regions with new pixels, saved once per sync (encoding a region is the slow part).
    for path in std::mem::take(&mut s.dirty) {
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        if let Some((_, data)) = s.regions.iter().find(|r| r.0 == path) {
            write_png(&path, data, REGION as u32, png::ColorType::Rgba)?;
        }
    }
    Ok(changed)
}

/// The saved map of a world and dimension: its folder (for loading the pictures), explored
/// regions and seed previews.
#[tauri::command]
pub fn mc_regions(app: AppHandle, world: String, dim: String) -> Result<Value, String> {
    let dir = dim_dir(&app, &world, &dim)?;
    let (mut explored, mut generated) = (vec![], vec![]);
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let parts: Vec<&str> = name.split('.').collect();
        if parts.len() != 4 || parts[3] != "png" {
            continue;
        }
        let (Ok(x), Ok(z)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>()) else {
            continue;
        };
        match parts[0] {
            "r" => explored.push([x, z]),
            "g" => generated.push([x, z]),
            _ => {}
        }
    }
    let _ = std::fs::create_dir_all(&dir);
    let _ = app.asset_protocol_scope().allow_directory(&dir, false);
    Ok(json!({ "dir": dir, "explored": explored, "generated": generated }))
}

/// Worlds with a saved map, newest first: id, name and dimensions.
#[tauri::command]
pub fn mc_worlds(app: AppHandle) -> Result<Vec<Value>, String> {
    let mut out = vec![];
    for e in std::fs::read_dir(root(&app)?).into_iter().flatten().flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let folder = e.file_name().to_string_lossy().to_string();
        let name = std::fs::read_to_string(p.join("name.txt")).unwrap_or_default();
        let dims: Vec<String> = std::fs::read_dir(&p)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|d| d.path().is_dir())
            .map(|d| d.file_name().to_string_lossy().to_string())
            .collect();
        let modified = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        out.push(json!({ "folder": folder, "name": name, "dims": dims, "modified": modified }));
    }
    out.sort_by_key(|v| std::cmp::Reverse(v["modified"].as_u64().unwrap_or(0)));
    Ok(out)
}

fn url_part(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"_-./".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Where the structures are in one region: [{t, set, x, z}]. Saved per region (s.X.Z.json) with the
/// answer's fingerprint (seed + the server's switches). The first region each run is asked fresh to
/// learn the current fingerprint; after that, saved regions are used while it matches, so a world
/// reset or an admin switching a kind off replaces them as they come up.
#[tauri::command]
pub async fn mc_structures(app: AppHandle, world: String, dim: String, rx: i32, rz: i32) -> Result<Value, String> {
    use std::sync::OnceLock;
    // World id -> fingerprint learned this run.
    static LEARNED: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    if rx.abs() > 1 << 16 || rz.abs() > 1 << 16 {
        return Err("Out of the world".into());
    }
    let file = dim_dir(&app, &world, &dim)?.join(format!("s.{rx}.{rz}.json"));
    tauri::async_runtime::spawn_blocking(move || {
        let learned = LEARNED.get_or_init(Default::default);
        let known = learned.lock().map_err(|_| "busy")?.get(&world).cloned();
        if let Some(known) = known.filter(|k| !k.is_empty()) {
            let saved = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
            if let Some(saved) = saved.filter(|v| v["rules"].as_str() == Some(known.as_str())) {
                return Ok(saved["list"].clone());
            }
        }
        let l = link().ok_or("HotSwap Link isn't installed")?;
        if status(Some(&l))["world"].as_str() != Some(world.as_str()) {
            return Err("Structures need this world open in Minecraft".to_string());
        }
        let (code, v) = get(&l, &format!("/structures?dim={}&rx={rx}&rz={rz}", url_part(&dim)), Duration::from_secs(45))?;
        if code == 404 {
            return Err("Update HotSwap Link to locate structures".into());
        }
        if code != 200 {
            return Err(v["error"].as_str().unwrap_or("Locating failed").to_string());
        }
        let found = json!(v["structures"]
            .as_array()
            .into_iter()
            .flatten()
            .take(512)
            .filter_map(|s| {
                Some(
                    json!({ "t": text(&s["t"], 64), "set": text(&s["set"], 64), "x": s["x"].as_i64()?, "z": s["z"].as_i64()?,
                    "pieces": s["pieces"].as_u64().unwrap_or(0), "height": s["height"].as_u64().unwrap_or(0), "tag": text(&s["tag"], 32) }),
                )
            })
            .collect::<Vec<_>>());
        // No fingerprint (an older mod): use it now, but don't save it.
        let rules = text(&v["rules"], 64);
        if !rules.is_empty() {
            learned.lock().map_err(|_| "busy")?.insert(world.clone(), rules.clone());
            std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
            let _ = std::fs::write(&file, json!({ "rules": rules, "list": found }).to_string());
        }
        Ok(found)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Asks the mod for a seed preview of one region (single-player only) and saves it.
#[tauri::command]
pub async fn mc_generate(app: AppHandle, world: String, dim: String, rx: i32, rz: i32) -> Result<(), String> {
    if rx.abs() > 1 << 16 || rz.abs() > 1 << 16 {
        return Err("Out of the world".into());
    }
    let dir = dim_dir(&app, &world, &dim)?;
    tauri::async_runtime::spawn_blocking(move || {
        let l = link().ok_or("HotSwap Link isn't installed")?;
        let st = status(Some(&l));
        if st["world"].as_str() != Some(world.as_str()) || st["canGenerate"] != true {
            return Err("Preview needs this world open in single-player".to_string());
        }
        let (code, v) = get(&l, &format!("/generate?dim={}&rx={rx}&rz={rz}", url_part(&dim)), Duration::from_secs(130))?;
        if code != 200 {
            return Err(v["error"].as_str().unwrap_or("Preview failed").to_string());
        }
        let rgb = base64::engine::general_purpose::STANDARD
            .decode(v["c"].as_str().unwrap_or(""))
            .map_err(|e| e.to_string())?;
        if rgb.len() != PREVIEW * PREVIEW * 3 {
            return Err("Bad preview".into());
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        write_png(&dir.join(format!("g.{rx}.{rz}.png")), &rgb, PREVIEW as u32, png::ColorType::Rgb)
    })
    .await
    .map_err(|e| e.to_string())?
}
