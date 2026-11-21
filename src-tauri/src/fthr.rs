//! FTHR Clips (fthrclips.com), the open-source clipping app, controlled from the overlay's Clips tab.
//! HotSwap's own FTHR plugin (coming with FTHR's plugin list) listens on 127.0.0.1 and writes its port
//! and a per-session token to ~/.fthr/hotswap-bridge.json. Without it, FTHR 2.1's Stream Deck
//! Bridge plugin works too (~/.fthr/stream-deck-bridge.json), with the same protocol: GET /status,
//! POST /command, each with a Bearer token. HotSwap's plugin adds the replay length and
//! POST /settings (FTHR's quick capture settings).
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
    /// HotSwap's own plugin (has /settings), not FTHR's Stream Deck one.
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
        return json!({
            "state": "ok",
            "plugin": if b.ours { "hotswap" } else { "streamdeck" },
            "ready": v["ready"].as_bool().unwrap_or(false),
            "capturing": v["capturing"].as_bool().unwrap_or(false),
            "recording": v["recording"].as_str().unwrap_or("idle"),
            "clipLength": v["clip_length"].as_f64(),
            // Only HotSwap's plugin 1.1+ sends these: the replay settings and their choices.
            "settings": if b.ours && v["settings"].is_object() { settings(&v["settings"]) } else { Value::Null },
        });
    }
    json!({ "state": "noplugin" })
}

/// The plugin's replay settings, passed on as plain values only.
fn settings(v: &Value) -> Value {
    let one = |x: &Value| match x {
        Value::Number(_) => x.clone(),
        Value::String(s) => json!(s.chars().take(16).collect::<String>()),
        _ => Value::Null,
    };
    let keys = ["clip_length", "framerate", "resolution", "quality"];
    json!({
        "current": keys.iter().map(|k| (k.to_string(), one(&v["current"][k]))).collect::<serde_json::Map<_, _>>(),
        "options": keys.iter().map(|k| (k.to_string(), json!(v["options"][k].as_array().into_iter().flatten().take(16).map(one).collect::<Vec<_>>()))).collect::<serde_json::Map<_, _>>(),
        "restartPending": v["restart_pending"].as_bool().unwrap_or(false),
    })
}

/// Changes one replay setting (`key` = clip_length/framerate/resolution/quality), or with
/// `apply` restarts capture so the changes take effect. The plugin checks the value is one of FTHR's.
#[tauri::command]
pub fn clips_settings(key: Option<String>, value: Option<Value>, apply: bool) -> Result<(), String> {
    let mut body = serde_json::Map::new();
    if let (Some(k), Some(v)) = (key, value) {
        if !["clip_length", "framerate", "resolution", "quality"].contains(&k.as_str()) {
            return Err("Unknown setting".into());
        }
        body.insert(k, v);
    }
    if apply {
        body.insert("apply".into(), json!(true));
    }
    if body.is_empty() {
        return Ok(());
    }
    let body = Value::Object(body).to_string();
    let b = bridges().into_iter().find(|b| b.ours).ok_or("Needs HotSwap's FTHR plugin 1.1")?;
    match request(&b, "POST", "/settings", Some(&body))? {
        (200, _) => {
            crate::logs::write("clips", &format!("FTHR Clips settings: {body}"));
            Ok(())
        }
        (404, _) => Err("Update HotSwap's FTHR plugin to change settings".into()),
        (code, v) => Err(v["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("FTHR answered {code}"))),
    }
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
