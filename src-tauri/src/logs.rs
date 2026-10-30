//! HotSwap's log (hotswap.log in the settings folder): what the engine and the interface did and
//! what went wrong, with readable times. It rotates at 5 MB (one older copy is kept), and
//! "Export logs" packs it into a zip, optionally trimmed to fit Discord's upload limit.

use std::{
    fs,
    io::{Seek, Write},
    path::PathBuf,
    sync::Mutex,
};

const ROTATE_AT: u64 = 5 << 20;
static LOCK: Mutex<()> = Mutex::new(());

pub fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("APPDATA").ok()?).join("app.hotswap.desktop"))
}

fn file() -> Option<PathBuf> {
    Some(dir()?.join("hotswap.log"))
}

/// The time now in UTC as "2026-10-30 18:04:05" (no time zone library needed).
fn now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rest / 3600, rest / 60 % 60, rest % 60)
}

/// Adds a line to the log: `area` says which part of HotSwap wrote it (app, ui, trainer, ...).
pub fn write(area: &str, msg: &str) {
    let Some(path) = file() else { return };
    let _guard = LOCK.lock();
    if fs::metadata(&path).is_ok_and(|m| m.len() > ROTATE_AT) {
        let _ = fs::rename(&path, path.with_extension("old.log"));
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} [{area}] {}", now(), msg.replace('\n', " | "));
    }
}

/// Logs crashes in the engine (Rust panics) with where they happened, before the default handler.
pub fn catch_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        write("crash", &format!("{msg} at {at} (thread {})", std::thread::current().name().unwrap_or("?")));
        default(info);
    }));
}

/// Packs the logs and a short system summary into a zip in Downloads (or the desktop) and returns
/// its path. With `small`, only the newest log lines go in, so the zip stays under 5 MB (what
/// fits in a Discord upload). The Discord login token and other secrets are never included.
pub fn export(small: bool, summary: &str) -> Result<PathBuf, String> {
    let dir = dir().ok_or("No settings folder")?;
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for name in ["hotswap.old.log", "hotswap.log", "trainer.log"] {
        if let Ok(b) = fs::read(dir.join(name)) {
            parts.push((name.to_string(), b));
        }
    }
    parts.push(("system.txt".into(), summary.as_bytes().to_vec()));
    let out_dir = std::env::var("USERPROFILE").map(|h| PathBuf::from(h).join("Downloads")).ok().filter(|d| d.is_dir()).or_else(|| std::env::var("USERPROFILE").ok().map(|h| PathBuf::from(h).join("Desktop"))).ok_or("No Downloads folder")?;
    let stamp = now().replace([' ', ':'], "-");
    let out = out_dir.join(format!("HotSwap-logs-{stamp}{}.zip", if small { "-discord" } else { "" }));
    // Logs compress about 10x; keep the newest part and shrink until the zip fits.
    let mut keep: usize = if small { 40 << 20 } else { usize::MAX };
    loop {
        let bytes = zip_parts(&parts, keep)?;
        if !small || bytes.len() <= (5 << 20) - 64 * 1024 || keep < 64 * 1024 {
            fs::write(&out, &bytes).map_err(|e| e.to_string())?;
            write("app", &format!("logs exported to {} ({} KB)", out.display(), bytes.len() / 1024));
            return Ok(out);
        }
        keep /= 2;
    }
}

/// A zip of the parts, each log cut to its newest `keep` bytes (from a line start).
fn zip_parts(parts: &[(String, Vec<u8>)], keep: usize) -> Result<Vec<u8>, String> {
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in parts {
            let mut start = data.len().saturating_sub(keep);
            if start > 0 {
                start += data[start..].iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
            }
            z.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
            if start > 0 {
                z.write_all(format!("[older lines left out to fit the size limit: {} KB]\n", start / 1024).as_bytes()).map_err(|e| e.to_string())?;
            }
            z.write_all(&data[start..]).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    cur.rewind().ok();
    Ok(cur.into_inner())
}
