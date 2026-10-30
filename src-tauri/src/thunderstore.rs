//! Thunderstore mods (BepInEx games such as R.E.P.O.): browse the community's package list, install
//! a mod with everything it depends on straight from Thunderstore, switch mods off and remove them.
//! Laid out like r2modman: BepInExPack in the game folder, each mod in BepInEx/plugins/<Owner-Name>.
//! Turned-off mods move to BepInEx/plugins_off. What HotSwap installed is listed in
//! BepInEx/hotswap_mods.json.

use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

/// The BepInEx package each community uses.
fn loader(community: &str) -> &'static str {
    match community {
        "subnautica" => "Subnautica_Modding-BepInExPack_Subnautica",
        "subnautica-below-zero" => "Subnautica_Modding-BepInExPack_BelowZero",
        _ => "BepInEx-BepInExPack",
    }
}

/// Every community's BepInEx package (to tell the loader apart from mods in a game folder).
fn is_loader(id: &str) -> bool {
    ["subnautica", "subnautica-below-zero", ""].iter().any(|c| loader(c) == id)
}

#[derive(Clone, Serialize)]
pub struct Package {
    /// Owner-Name, the ID dependencies use.
    id: String,
    name: String,
    owner: String,
    description: String,
    icon: String,
    downloads: u64,
    rating: u64,
    version: String,
    updated: String,
    page: String,
    #[serde(skip)]
    url: String,
    #[serde(skip)]
    deps: Vec<String>,
}

#[derive(Serialize)]
pub struct Installed {
    id: String,
    name: String,
    version: String,
    enabled: bool,
    loader: bool,
}

/// Package lists per community, kept for a while (they're big).
static CACHE: Mutex<Option<HashMap<String, (Instant, Vec<Package>)>>> = Mutex::new(None);

fn packages(community: &str) -> Result<Vec<Package>, String> {
    if let Some((at, list)) = CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(community) {
        if at.elapsed() < Duration::from_secs(1800) {
            return Ok(list.clone());
        }
    }
    let v: Value = ureq::get(&format!("https://thunderstore.io/c/{community}/api/v1/package/"))
        .timeout(Duration::from_secs(60))
        .call()
        .map_err(|e| format!("Couldn't reach Thunderstore: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let list: Vec<Package> = v
        .as_array()
        .ok_or("Thunderstore sent no list")?
        .iter()
        .filter(|p| !p["is_deprecated"].as_bool().unwrap_or(false) && !p["has_nsfw_content"].as_bool().unwrap_or(false))
        .filter_map(|p| {
            let vs = p["versions"].as_array()?;
            let last = vs.first()?;
            Some(Package {
                id: p["full_name"].as_str()?.to_string(),
                name: p["name"].as_str()?.replace('_', " "),
                owner: p["owner"].as_str().unwrap_or("").to_string(),
                description: last["description"].as_str().unwrap_or("").to_string(),
                icon: last["icon"].as_str().unwrap_or("").to_string(),
                downloads: vs.iter().map(|x| x["downloads"].as_u64().unwrap_or(0)).sum(),
                rating: p["rating_score"].as_u64().unwrap_or(0),
                version: last["version_number"].as_str().unwrap_or("").to_string(),
                updated: p["date_updated"].as_str().unwrap_or("").to_string(),
                page: p["package_url"].as_str().unwrap_or("").to_string(),
                url: last["download_url"].as_str()?.to_string(),
                // "Owner-Name-1.2.3" -> "Owner-Name"
                deps: last["dependencies"].as_array().map(|d| d.iter().filter_map(|x| x.as_str()).map(|s| s.rsplitn(2, '-').nth(1).unwrap_or(s).to_string()).collect()).unwrap_or_default(),
            })
        })
        .collect();
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(community.to_string(), (Instant::now(), list.clone()));
    Ok(list)
}

/// Forgets the cached package lists (when the Mods page is closed).
pub fn release() {
    *CACHE.lock().unwrap() = None;
}

/// One page of mods matching `query`, sorted by popular, updated, new or rating.
pub fn search(community: &str, query: &str, sort: &str, offset: usize) -> Result<Value, String> {
    let q = query.to_lowercase();
    let mut list: Vec<Package> = packages(community)?
        .into_iter()
        .filter(|p| !is_loader(&p.id) && (q.is_empty() || p.name.to_lowercase().contains(&q) || p.owner.to_lowercase().contains(&q) || p.description.to_lowercase().contains(&q)))
        .collect();
    match sort {
        "updated" | "new" => list.sort_by(|a, b| b.updated.cmp(&a.updated)),
        "rating" => list.sort_by(|a, b| b.rating.cmp(&a.rating)),
        _ => list.sort_by(|a, b| b.downloads.cmp(&a.downloads)),
    }
    let total = list.len();
    let page: Vec<Package> = list.into_iter().skip(offset).take(30).collect();
    Ok(json!({ "nodes": page, "totalCount": total }))
}

pub struct Game {
    pub root: PathBuf,
}

impl Game {
    fn bep(&self) -> PathBuf {
        self.root.join("BepInEx")
    }
    fn index_path(&self) -> PathBuf {
        self.bep().join("hotswap_mods.json")
    }
    fn index(&self) -> serde_json::Map<String, Value> {
        std::fs::read_to_string(self.index_path()).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
    }
    fn save_index(&self, idx: &serde_json::Map<String, Value>) -> Result<(), String> {
        std::fs::create_dir_all(self.bep()).map_err(|e| e.to_string())?;
        std::fs::write(self.index_path(), serde_json::to_string_pretty(idx).unwrap()).map_err(|e| e.to_string())
    }
    fn on_dir(&self, id: &str) -> PathBuf {
        self.bep().join("plugins").join(id)
    }
    fn off_dir(&self, id: &str) -> PathBuf {
        self.bep().join("plugins_off").join(id)
    }

    pub fn list(&self) -> Vec<Installed> {
        let mut out: Vec<Installed> = self
            .index()
            .iter()
            .map(|(id, v)| Installed {
                id: id.clone(),
                name: v["name"].as_str().unwrap_or(id).to_string(),
                version: v["version"].as_str().unwrap_or("").to_string(),
                enabled: is_loader(id) || !self.off_dir(id).exists(),
                loader: is_loader(id),
            })
            .collect();
        out.sort_by(|a, b| b.loader.cmp(&a.loader).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        out
    }

    pub fn set_enabled(&self, id: &str, on: bool) -> Result<(), String> {
        let (from, to) = if on { (self.off_dir(id), self.on_dir(id)) } else { (self.on_dir(id), self.off_dir(id)) };
        if !from.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::rename(&from, &to).map_err(|e| e.to_string())
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        if is_loader(id) {
            return Err("BepInEx is the mod loader; remove the other mods instead".into());
        }
        for d in [self.on_dir(id), self.off_dir(id), self.bep().join("patchers").join(id)] {
            if d.exists() {
                std::fs::remove_dir_all(&d).map_err(|e| e.to_string())?;
            }
        }
        let mut idx = self.index();
        idx.remove(id);
        self.save_index(&idx)
    }

    /// Installs `id` and everything it needs (BepInEx included), skipping what's already there
    /// unless it's `id` itself (which updates it). Returns the names installed.
    pub fn install(&self, community: &str, id: &str) -> Result<Vec<String>, String> {
        let all = packages(community)?;
        let by_id: HashMap<&str, &Package> = all.iter().map(|p| (p.id.as_str(), p)).collect();
        // Dependencies first, depth first.
        let mut order: Vec<&Package> = Vec::new();
        fn walk<'a>(id: &str, by_id: &HashMap<&str, &'a Package>, order: &mut Vec<&'a Package>, depth: u32) -> Result<(), String> {
            if depth > 20 || order.iter().any(|p| p.id == id) {
                return Ok(());
            }
            let p = by_id.get(id).ok_or(format!("{id} isn't on Thunderstore"))?;
            for d in &p.deps {
                walk(d, by_id, order, depth + 1)?;
            }
            order.push(p);
            Ok(())
        }
        walk(id, &by_id, &mut order, 0)?;
        let base = loader(community);
        if !order.iter().any(|p| p.id == base) && !self.index().contains_key(base) {
            if let Some(l) = by_id.get(base) {
                order.insert(0, l);
            }
        }
        let mut idx = self.index();
        let mut done = Vec::new();
        for p in order {
            if p.id != id && idx.get(&p.id).and_then(|v| v["version"].as_str()) == Some(p.version.as_str()) {
                continue;
            }
            self.unpack(p)?;
            idx.insert(p.id.clone(), json!({ "name": p.name, "version": p.version }));
            self.save_index(&idx)?;
            done.push(p.name.clone());
        }
        Ok(done)
    }

    fn unpack(&self, p: &Package) -> Result<(), String> {
        let mut bytes = Vec::new();
        ureq::get(&p.url)
            .timeout(Duration::from_secs(300))
            .call()
            .map_err(|e| format!("Couldn't download {}: {e}", p.name))?
            .into_reader()
            .take(500 << 20)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("{} isn't a zip: {e}", p.name))?;
        let loader = is_loader(&p.id);
        let target = if loader { self.root.clone() } else { self.on_dir(&p.id) };
        if !loader {
            let _ = std::fs::remove_dir_all(self.off_dir(&p.id));
            let _ = std::fs::remove_dir_all(&target);
        }
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
            if f.is_dir() {
                continue;
            }
            let Some(rel) = f.enclosed_name() else { continue };
            let rel: PathBuf = rel.components().collect();
            let out = if loader {
                // BepInExPack*/... goes into the game folder; the package's own files are skipped.
                let mut parts = rel.components();
                match parts.next() {
                    Some(c) if c.as_os_str().to_string_lossy().starts_with("BepInExPack") => target.join(parts.as_path()),
                    _ => continue,
                }
            } else {
                place(&self.bep(), &target, &p.id, &rel)
            };
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let mut w = std::fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            std::io::copy(&mut f, &mut w).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// Where a file from a mod's zip goes: plugins/ and loose files into the mod's folder, config/
/// into BepInEx/config, patchers/ into BepInEx/patchers/<id> (as r2modman does).
/// BepInEx 5 (x64) from its GitHub releases, for games without a Thunderstore pack.
const BEPINEX_ZIP: &str = "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.2/BepInEx_win_x64_5.4.23.2.zip";

impl Game {
    /// Makes sure BepInEx is in the game folder: the community's pack, or plain BepInEx.
    fn ensure_loader(&self, community: &str) -> Result<(), String> {
        if self.bep().join("core").is_dir() {
            return Ok(());
        }
        if !community.is_empty() {
            self.install(community, loader(community))?;
            return Ok(());
        }
        let mut bytes = Vec::new();
        ureq::get(BEPINEX_ZIP).timeout(Duration::from_secs(120)).call().map_err(|e| format!("Couldn't download BepInEx: {e}"))?
            .into_reader().take(100 << 20).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        zip.extract(&self.root).map_err(|e| format!("Couldn't unpack BepInEx: {e}"))?;
        let mut idx = self.index();
        idx.insert("BepInEx-BepInExPack".into(), json!({ "name": "BepInEx", "version": "5.4.23.2" }));
        self.save_index(&idx)
    }

    /// Installs a mod downloaded by hand (a .zip or .7z from Nexus Mods), BepInEx first if needed.
    pub fn install_file(&self, community: &str, file: &Path, scratch: &Path) -> Result<String, String> {
        self.ensure_loader(community)?;
        let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "mod".into());
        // Nexus names files like "Mod Name-123-1-0-1700000000": keep the name part.
        let name = stem.split('-').next().unwrap_or(&stem).trim().to_string();
        let id = format!("nexus-{}", name.replace(|c: char| !c.is_ascii_alphanumeric(), "_"));
        let tmp = scratch.join("bepunpack");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        match file.extension().map(|x| x.to_string_lossy().to_lowercase()).as_deref() {
            Some("zip") => crate::modfiles::unzip(file, &tmp)?,
            Some("7z") => sevenz_rust::decompress_file(file, &tmp).map_err(|e| format!("7z: {e}"))?,
            Some("dll") => {
                std::fs::copy(file, tmp.join(file.file_name().unwrap())).map_err(|e| e.to_string())?;
            }
            _ => return Err("Pick a .zip, .7z or .dll file".into()),
        }
        let target = self.on_dir(&id);
        let _ = std::fs::remove_dir_all(self.off_dir(&id));
        let _ = std::fs::remove_dir_all(&target);
        for f in crate::modfiles::walk(&tmp) {
            let Ok(rel) = f.strip_prefix(&tmp) else { continue };
            // Anything under a BepInEx folder (however deep) keeps its place in BepInEx.
            let parts: Vec<_> = rel.components().collect();
            let rel: PathBuf = match parts.iter().position(|c| c.as_os_str().eq_ignore_ascii_case("BepInEx")) {
                Some(i) => parts[i..].iter().collect(),
                None => rel.to_path_buf(),
            };
            let out = place(&self.bep(), &target, &id, &rel);
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::copy(&f, &out).map_err(|e| format!("{}: {e}", out.display()))?;
        }
        let _ = std::fs::remove_dir_all(&tmp);
        let mut idx = self.index();
        idx.insert(id, json!({ "name": name, "version": "from Nexus" }));
        self.save_index(&idx)?;
        Ok(name)
    }
}

fn place(bep: &Path, target: &Path, id: &str, rel: &Path) -> PathBuf {
    let mut parts = rel.components();
    let first = parts.next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).unwrap_or_default();
    let rest: PathBuf = parts.collect();
    match first.as_str() {
        "bepinex" => place(bep, target, id, &rest),
        "plugins" if !rest.as_os_str().is_empty() => target.join(rest),
        "config" if !rest.as_os_str().is_empty() => bep.join("config").join(rest),
        "patchers" if !rest.as_os_str().is_empty() => bep.join("patchers").join(id).join(rest),
        _ => target.join(rel),
    }
}
