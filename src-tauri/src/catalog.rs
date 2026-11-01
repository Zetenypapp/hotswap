//! Mod lists that arrive without an app update: catalog/mods.json in the repository, signed with
//! the same key as HotSwap's updates (`cargo tauri signer sign`). HotSwap downloads it, checks the
//! signature against the key built into the app, refuses an older list than the one it has, and
//! drops any mod that would run code (`js`), so a list can only describe data: which values to
//! change. The verified list is kept, so it also works offline.

use serde_json::Value;
use std::{path::PathBuf, time::Duration};
use tauri::{AppHandle, Manager};

const URL: &str = "https://raw.githubusercontent.com/Zetenypapp/hotswap/main/catalog/mods.json";

fn cache_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok()
}

/// HotSwap's update key, from tauri.conf.json (base64 of a minisign public key file).
fn key_of(app: &AppHandle) -> String {
    app.config().plugins.0.get("updater").and_then(|u| u.get("pubkey")).and_then(|k| k.as_str()).unwrap_or("").to_string()
}

fn verify(app: &AppHandle, data: &[u8], sig_b64: &str) -> Result<(), String> {
    verify_with(&key_of(app), data, sig_b64)
}

/// Checks `data` against a Tauri-style signature (base64 of a minisign signature file).
pub fn verify_with(key_b64: &str, data: &[u8], sig_b64: &str) -> Result<(), String> {
    let pk = String::from_utf8(base64_decode(key_b64).unwrap_or_default()).ok()
        .and_then(|t| minisign_verify::PublicKey::decode(&t).ok())
        .ok_or("no signing key in the app")?;
    let text = String::from_utf8(base64_decode(sig_b64.trim()).ok_or("bad signature encoding")?).map_err(|_| "bad signature")?;
    let sig = minisign_verify::Signature::decode(&text).map_err(|e| format!("bad signature: {e}"))?;
    pk.verify(data, &sig, false).map_err(|_| "the signature doesn't match".to_string())
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        acc = (acc << 6) | T.iter().position(|&t| t == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Removes mods that would run code: a downloaded list may only describe values to change.
fn data_only(v: &mut Value) {
    let runs_code = |m: &Value| {
        let specs = match &m["spec"] {
            Value::Array(a) => a.clone(),
            s @ Value::Object(_) => vec![s.clone()],
            _ => vec![],
        };
        specs.iter().any(|s| s.get("js").is_some() || s.get("off").is_some())
    };
    let clean_cats = |cats: &mut Value| {
        for cat in cats.as_array_mut().into_iter().flatten() {
            if let Some(mods) = cat.get_mut("mods").and_then(|m| m.as_array_mut()) {
                mods.retain(|m| !runs_code(m));
            }
        }
    };
    for g in v.get_mut("games").and_then(|g| g.as_object_mut()).into_iter().flat_map(|g| g.values_mut()) {
        if let Some(cats) = g.get_mut("cats") {
            clean_cats(cats);
        }
    }
    for cats in v.get_mut("engines").and_then(|e| e.as_object_mut()).into_iter().flat_map(|e| e.values_mut()) {
        clean_cats(cats);
    }
}

fn version_of(v: &Value) -> u64 {
    v["version"].as_u64().unwrap_or(0)
}

/// The verified list saved last time, if any.
fn cached(app: &AppHandle) -> Option<Value> {
    let dir = cache_dir(app)?;
    let data = std::fs::read(dir.join("catalog.json")).ok()?;
    let sig = std::fs::read_to_string(dir.join("catalog.json.sig")).ok()?;
    verify(app, &data, &sig).ok()?;
    serde_json::from_slice(&data).ok()
}

/// Downloads the newest list; saves and returns it if it's signed and not older than the saved one.
fn download(app: &AppHandle) -> Result<Option<Value>, String> {
    let get = |url: &str| -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        use std::io::Read;
        ureq::get(url).timeout(Duration::from_secs(15)).call().map_err(|e| e.to_string())?
            .into_reader().take(4 << 20).read_to_end(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    };
    let data = get(URL)?;
    let sig = String::from_utf8(get(&format!("{URL}.sig"))?).map_err(|_| "bad signature file")?;
    verify(app, &data, &sig)?;
    let v: Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if cached(app).is_some_and(|c| version_of(&c) > version_of(&v)) {
        return Ok(None);
    }
    if let Some(dir) = cache_dir(app) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("catalog.json"), &data);
        let _ = std::fs::write(dir.join("catalog.json.sig"), &sig);
    }
    Ok(Some(v))
}

/// The downloaded mod list for the UI: the saved one, or (with `refresh`) the newest one online.
/// Only signed lists are ever returned, and never with mods that run code.
#[tauri::command]
pub(crate) async fn catalog_remote(app: AppHandle, refresh: bool) -> Option<Value> {
    let mut v = if refresh {
        match tauri::async_runtime::spawn_blocking({
            let app = app.clone();
            move || download(&app)
        })
        .await
        {
            Ok(Ok(Some(v))) => Some(v),
            Ok(Err(e)) => {
                crate::logs::write("app", &format!("mod list update: {e}"));
                None
            }
            _ => None,
        }
    } else {
        cached(&app)
    }?;
    data_only(&mut v);
    Some(v)
}
