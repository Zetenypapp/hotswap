//! SpaceDock mods for Kerbal Space Program: browse spacedock.info, install a mod's zip into
//! GameData, switch mods off (their folders move to GameData_off) and remove them. SpaceDock has no
//! dependency data, so mods that need others say so on their page. What HotSwap installed, and
//! which GameData entries each mod added, is listed in GameData/hotswap_mods.json.
//! Same shapes as thunderstore.rs, so the mod page treats both alike.

use serde_json::{json, Value};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

const SITE: &str = "https://spacedock.info";
/// SpaceDock's ID for KSP 1 (it hosts KSP 2 mods too).
const KSP1: i64 = 3102;

fn get(url: &str) -> Result<Value, String> {
    ureq::get(url).timeout(Duration::from_secs(30)).call().map_err(|e| format!("Couldn't reach SpaceDock: {e}"))?.into_json().map_err(|e| e.to_string())
}

fn package(m: &Value) -> Option<Value> {
    let v = m["versions"].as_array()?.first()?;
    Some(json!({
        "id": m["id"].as_i64()?.to_string(),
        "name": m["name"],
        "owner": m["author"],
        "description": m["short_description"],
        "icon": m["background"].as_str().unwrap_or(""),
        "downloads": m["downloads"],
        "version": v["friendly_version"],
        "game_version": v["game_version"],
        "page": format!("{SITE}{}", m["url"].as_str().unwrap_or("")),
    }))
}

/// One page (30) of mods: popular, new or updated when there's no query, else a search.
pub fn search(query: &str, sort: &str, offset: usize) -> Result<Value, String> {
    let page = offset / 30 + 1;
    let q = query.trim();
    let list = if q.is_empty() {
        let kind = match sort { "new" | "updated" => "new", _ => "top" };
        get(&format!("{SITE}/api/browse/{kind}?page={page}"))?
    } else {
        get(&format!("{SITE}/api/search/mod?query={}&page={page}", urlencode(q)))?
    };
    let arr = list.as_array().cloned().unwrap_or_default();
    let full = arr.len() >= 30;
    let nodes: Vec<Value> = arr.iter().filter(|m| m["game_id"].as_i64() == Some(KSP1)).filter_map(package).collect();
    // SpaceDock doesn't say how many there are: offer "Load more" while pages come back full.
    let shown = offset + nodes.len();
    Ok(json!({ "nodes": nodes, "totalCount": if full { shown + 30 } else { shown } }))
}

fn urlencode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

pub struct Game {
    pub root: PathBuf,
}

impl Game {
    fn data(&self) -> PathBuf {
        self.root.join("GameData")
    }
    fn off(&self) -> PathBuf {
        self.root.join("GameData_off")
    }
    fn index_path(&self) -> PathBuf {
        self.data().join("hotswap_mods.json")
    }
    fn index(&self) -> serde_json::Map<String, Value> {
        std::fs::read_to_string(self.index_path()).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
    }
    fn save_index(&self, idx: &serde_json::Map<String, Value>) -> Result<(), String> {
        std::fs::write(self.index_path(), serde_json::to_string_pretty(idx).unwrap()).map_err(|e| e.to_string())
    }
    fn entries(v: &Value) -> Vec<String> {
        v["entries"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
    }

    pub fn list(&self) -> Value {
        let mut out: Vec<Value> = self
            .index()
            .iter()
            .map(|(id, v)| {
                let on = Self::entries(v).iter().all(|e| !self.off().join(e).exists());
                json!({ "id": id, "name": v["name"], "version": v["version"], "enabled": on, "loader": false })
            })
            .collect();
        out.sort_by_key(|m| m["name"].as_str().unwrap_or("").to_lowercase());
        json!(out)
    }

    pub fn set_enabled(&self, id: &str, on: bool) -> Result<(), String> {
        let idx = self.index();
        let v = idx.get(id).ok_or("Not installed by HotSwap")?;
        let (from, to) = if on { (self.off(), self.data()) } else { (self.data(), self.off()) };
        std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
        for e in Self::entries(v) {
            if from.join(&e).exists() && !to.join(&e).exists() {
                std::fs::rename(from.join(&e), to.join(&e)).map_err(|err| format!("{e}: {err}"))?;
            }
        }
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut idx = self.index();
        let v = idx.remove(id).ok_or("Not installed by HotSwap")?;
        // Entries another installed mod also uses (shared libraries) stay.
        let shared: Vec<String> = idx.values().flat_map(Self::entries).collect();
        for e in Self::entries(&v).into_iter().filter(|e| !shared.contains(e)) {
            for base in [self.data(), self.off()] {
                let p = base.join(&e);
                let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
            }
        }
        self.save_index(&idx)
    }

    /// Installs (or updates) a mod by its SpaceDock ID. Returns its name.
    pub fn install(&self, id: &str) -> Result<Vec<String>, String> {
        if !self.data().is_dir() {
            return Err("GameData folder not found".into());
        }
        let m = get(&format!("{SITE}/api/mod/{id}"))?;
        let name = m["name"].as_str().unwrap_or(id).to_string();
        let v = m["versions"].as_array().and_then(|a| a.first()).ok_or("No downloadable version")?;
        let mut bytes = Vec::new();
        ureq::get(&format!("{SITE}{}", v["download_path"].as_str().unwrap_or("")))
            .timeout(Duration::from_secs(600))
            .call()
            .map_err(|e| format!("Couldn't download {name}: {e}"))?
            .into_reader()
            .take(2 << 30)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("{name} isn't a zip: {e}"))?;
        // Files under a GameData folder go into GameData; a zip without one is all GameData.
        let has_gd = (0..zip.len()).any(|i| zip.by_index(i).ok().and_then(|f| f.enclosed_name()).is_some_and(|p| gamedata_rel(&p).is_some()));
        let mut entries: Vec<String> = Vec::new();
        if let Some(old) = self.index().get(id) {
            // An update: the old version's files go first.
            for e in Self::entries(old) {
                let _ = std::fs::remove_dir_all(self.data().join(&e));
                let _ = std::fs::remove_dir_all(self.off().join(&e));
            }
        }
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
            if f.is_dir() {
                continue;
            }
            let Some(p) = f.enclosed_name() else { continue };
            let rel = if has_gd { match gamedata_rel(&p) { Some(r) => r, None => continue } } else { p };
            let Some(top) = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_string()) else { continue };
            if !entries.contains(&top) {
                entries.push(top);
            }
            let out = self.data().join(&rel);
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let mut w = std::fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            std::io::copy(&mut f, &mut w).map_err(|e| e.to_string())?;
        }
        let mut idx = self.index();
        idx.insert(id.to_string(), json!({ "name": name, "version": v["friendly_version"], "entries": entries }));
        self.save_index(&idx)?;
        Ok(vec![name])
    }
}

/// The part of a zip path after its GameData folder, if it has one.
fn gamedata_rel(p: &Path) -> Option<PathBuf> {
    let parts: Vec<_> = p.components().collect();
    let i = parts.iter().position(|c| c.as_os_str().eq_ignore_ascii_case("GameData"))?;
    let rest: PathBuf = parts[i + 1..].iter().collect();
    (!rest.as_os_str().is_empty()).then_some(rest)
}
