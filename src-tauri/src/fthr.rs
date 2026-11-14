//! FTHR Clips (fthrclips.com), the open-source clipping app, controlled from the overlay's Clips tab.
//! HotSwap's own FTHR plugin (coming with FTHR's plugin list) listens on 127.0.0.1 and writes its port
//! and a per-session token to ~/.fthr/hotswap-bridge.json. Without it, FTHR 2.1's Stream Deck
//! Bridge plugin works too (~/.fthr/stream-deck-bridge.json), with the same protocol: GET /status,
//! POST /command, each with a Bearer token. HotSwap's plugin adds GET /recent (latest saved files)
//! and the replay length.
//! Nothing here touches FTHR's capture engine: every command goes through FTHR's own guarded handlers.

use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    time::Duration,
};

/// The commands the bridge accepts (its COMMANDS table).
const COMMANDS: [&str; 4] = ["save_clip", "mark_highlight", "toggle_recording", "save_screenshot"];

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

fn installed_exe() -> Option<PathBuf> {
    let mut dirs = vec![];
    for v in ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"] {
        if let Some(d) = std::env::var_os(v) {
            dirs.push(PathBuf::from(&d).join("FTHRClips"));
            dirs.push(PathBuf::from(&d).join(r"Programs\FTHRClips"));
        }
    }
    dirs.into_iter().map(|d| d.join("FTHRClips.exe")).find(|p| p.is_file())
}

fn running() -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes().values().any(|p| p.name().eq_ignore_ascii_case("FTHRClips.exe"))
}

struct Bridge {
    port: u16,
    token: String,
    /// HotSwap's own plugin (has /recent), not FTHR's Stream Deck one.
    ours: bool,
}

/// HotSwap's plugin first, then FTHR's Stream Deck Bridge.
fn bridges() -> Vec<Bridge> {
    [("hotswap-bridge.json", true), ("stream-deck-bridge.json", false)].into_iter().filter_map(|(f, ours)| discovery(f, ours)).collect()
}

fn discovery(file: &str, ours: bool) -> Option<Bridge> {
    let text = std::fs::read_to_string(home()?.join(".fthr").join(file)).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    if v["protocol"].as_u64() != Some(1) {
        return None;
    }
    let port = u16::try_from(v["port"].as_u64()?).ok().filter(|&p| p > 0)?;
    let token = v["token"].as_str()?.to_string();
    // The token goes into a header: only the URL-safe characters FTHR makes it from.
    if token.is_empty() || token.len() > 128 || !token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return None;
    }
    Some(Bridge { port, token, ours })
}

/// One HTTP/1.1 request to the bridge. It closes the connection after each answer.
fn request(b: &Bridge, method: &str, path: &str, body: Option<&str>) -> Result<(u16, Value), String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], b.port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(400)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_millis(1500))).ok();
    s.set_write_timeout(Some(Duration::from_millis(1500))).ok();
    let body = body.unwrap_or("");
    let ctype = if method == "POST" { "Content-Type: application/json\r\n" } else { "" };
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\n{ctype}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        b.port,
        b.token,
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    s.take(64 * 1024).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buf);
    let (head, rest) = text.split_once("\r\n\r\n").ok_or("Bad answer from FTHR")?;
    let code = head.split(' ').nth(1).and_then(|c| c.parse().ok()).ok_or("Bad answer from FTHR")?;
    Ok((code, serde_json::from_str(rest).unwrap_or(Value::Null)))
}

/// What the Clips tab shows. `state` is one of:
/// - "missing": FTHR Clips isn't installed
/// - "closed": installed, not running
/// - "noplugin": running, but the Stream Deck Bridge plugin isn't installed or enabled
/// - "ok": connected (then `ready`, `capturing` and `recording` come from FTHR)
#[tauri::command]
pub fn clips_status() -> Value {
    let exe = installed_exe();
    let up = running();
    if !up {
        return json!({ "state": if exe.is_some() { "closed" } else { "missing" } });
    }
    // A file can be left from an earlier run, or its plugin was just turned off: the first that answers.
    for b in bridges() {
        let Ok((200, v)) = request(&b, "GET", "/status", None) else { continue };
        let recent = if b.ours { request(&b, "GET", "/recent", None).ok().filter(|r| r.0 == 200).map(|r| recent_files(&r.1["files"])) } else { None };
        return json!({
            "state": "ok",
            "plugin": if b.ours { "hotswap" } else { "streamdeck" },
            "ready": v["ready"].as_bool().unwrap_or(false),
            "capturing": v["capturing"].as_bool().unwrap_or(false),
            "recording": v["recording"].as_str().unwrap_or("idle"),
            "clipLength": v["clip_length"].as_f64(),
            "recent": recent,
        });
    }
    json!({ "state": "noplugin" })
}

/// The plugin's latest files, passed on with only the fields the overlay shows.
fn recent_files(v: &Value) -> Vec<Value> {
    let text = |x: &Value| x.as_str().unwrap_or("").chars().take(160).collect::<String>();
    v.as_array().into_iter().flatten().take(8).map(|f| json!({
        "name": text(&f["name"]),
        "group": text(&f["group"]),
        "kind": match f["kind"].as_str() { Some(k @ ("clip" | "recording" | "screenshot")) => k, _ => "clip" },
        "size": f["size"].as_u64().unwrap_or(0),
        "modified": f["modified"].as_u64().unwrap_or(0),
    })).collect()
}

#[tauri::command]
pub fn clips_command(command: String) -> Result<(), String> {
    if !COMMANDS.contains(&command.as_str()) {
        return Err("Unknown command".into());
    }
    let body = json!({ "command": command }).to_string();
    // The first bridge that's listening; a refused connection means a stale file, so try the next.
    let mut answer = Err("FTHR's HotSwap plugin isn't on".to_string());
    for b in bridges() {
        answer = request(&b, "POST", "/command", Some(&body));
        if answer.is_ok() {
            break;
        }
    }
    match answer? {
        (200, _) => {
            crate::logs::write("clips", &format!("FTHR Clips: {command}"));
            Ok(())
        }
        (503, _) => Err("FTHR isn't ready to capture yet".into()),
        (code, v) => Err(v["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("FTHR answered {code}"))),
    }
}

/// Starts FTHR Clips when it's installed but closed.
#[tauri::command]
pub fn clips_open() -> Result<(), String> {
    let exe = installed_exe().ok_or("FTHR Clips isn't installed")?;
    std::process::Command::new(exe).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Dev aid: `hotswap --fthr [command]` prints the status, or sends a command.
pub fn test(cmd: Option<String>) {
    match cmd {
        Some(c) => println!("{:?}", clips_command(c)),
        None => println!("{}", clips_status()),
    }
}
