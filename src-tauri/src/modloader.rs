//! Satisfactory mod loader: browses ficsit.app (the community mod repository), installs mods with
//! their dependencies (including the Satisfactory Mod Loader itself), and enables, disables,
//! updates and removes them. Uses the same folder layout as Satisfactory Mod Manager
//! (FactoryGame/Mods/<ModReference>), so the two can be used side by side.

use semver::{Version, VersionReq};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const API: &str = "https://api.ficsit.app";

#[derive(Serialize, Clone)]
pub struct Installed {
    reference: String,
    name: String,
    version: String,
    enabled: bool,
}

#[derive(Serialize, Clone)]
pub struct Progress {
    pub reference: String,
    pub stage: &'static str,
    pub done: u64,
    pub total: u64,
}

fn mods_dir(install: &Path) -> PathBuf {
    install.join("FactoryGame").join("Mods")
}

/// Disabled mods are moved here, outside the folder the game loads from.
fn disabled_dir(install: &Path) -> PathBuf {
    install.join("FactoryGame").join("Mods (disabled by Hotswap)")
}

/// The game's build number (changelist), which mod versions declare compatibility with.
pub fn game_build(install: &Path) -> Option<u64> {
    let bin = install.join("Engine").join("Binaries").join("Win64");
    fs::read_dir(bin).ok()?.flatten().find_map(|e| {
        let p = e.path();
        if p.extension()? != "version" {
            return None;
        }
        let v: Value = serde_json::from_str(fs::read_to_string(&p).ok()?.trim_start_matches('\u{feff}')).ok()?;
        v["Changelist"].as_u64()
    })
}

/// Mods in the game folder (enabled) and in the disabled folder.
pub fn installed(install: &Path) -> Vec<Installed> {
    let mut out = Vec::new();
    for (dir, enabled) in [(mods_dir(install), true), (disabled_dir(install), false)] {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let reference = e.file_name().to_string_lossy().to_string();
            let plugin = e.path().join(format!("{reference}.uplugin"));
            let Ok(text) = fs::read_to_string(&plugin) else { continue };
            let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default();
            let version = v["SemVersion"].as_str().or(v["VersionName"].as_str()).unwrap_or("").to_string();
            let name = v["FriendlyName"].as_str().unwrap_or(&reference).to_string();
            out.push(Installed { reference, name, version, enabled });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn graphql(query: &str, variables: Value) -> Result<Value, String> {
    let r: Value = ureq::post(&format!("{API}/v2/query"))
        .timeout(std::time::Duration::from_secs(20))
        .send_json(json!({ "query": query, "variables": variables }))
        .map_err(|e| format!("ficsit.app: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    if let Some(err) = r["errors"].as_array().and_then(|a| a.first()) {
        return Err(format!("ficsit.app: {}", err["message"].as_str().unwrap_or("error")));
    }
    Ok(r["data"].clone())
}

/// One page of mods from ficsit.app. `sort` is "downloads", "last_version_date" or "created_at".
pub fn search(query: &str, sort: &str, offset: u32) -> Result<Value, String> {
    let order_by = match sort {
        "updated" => "last_version_date",
        "new" => "created_at",
        _ => "downloads",
    };
    let mut filter = json!({ "limit": 30, "offset": offset, "order_by": order_by, "order": "desc", "hidden": false });
    if !query.trim().is_empty() {
        filter["search"] = json!(query.trim());
        filter.as_object_mut().unwrap().remove("order_by");
    }
    let d = graphql(
        "query($f: ModFilter) { getMods(filter: $f) { count mods { mod_reference name short_description downloads logo authors { user { username } }
           compatibility { EA { state } EXP { state } } last_version_date latestVersions { release { version } } } } }",
        json!({ "f": filter }),
    )?;
    Ok(d["getMods"].clone())
}

#[derive(Clone)]
struct Release {
    version: Version,
    link: String,
    hash: String,
    size: u64,
    deps: Vec<(String, VersionReq)>,
}

/// Every release of a mod that has a Windows download and supports this game build, newest first.
fn releases(reference: &str, build: Option<u64>) -> Result<Vec<Release>, String> {
    let d = graphql(
        "query($r: ModReference!) { getModByReference(modReference: $r) { versions(filter: { limit: 100, order_by: created_at, order: desc }) {
           version game_version targets { targetName link hash size } dependencies { mod_id condition optional } } } }",
        json!({ "r": reference }),
    )?;
    let versions = d["getModByReference"]["versions"].as_array().ok_or(format!("{reference} isn't on ficsit.app"))?;
    let mut out: Vec<Release> = versions
        .iter()
        .filter_map(|v| {
            let version = Version::parse(v["version"].as_str()?).ok()?;
            if let (Some(b), Some(req)) = (build, v["game_version"].as_str()) {
                if !game_ok(req, b) {
                    return None;
                }
            }
            let t = v["targets"].as_array()?.iter().find(|t| t["targetName"] == "Windows")?;
            let deps = v["dependencies"].as_array().map(|a| a.iter().filter(|d| d["optional"] != true)
                .filter_map(|d| Some((d["mod_id"].as_str()?.to_string(), VersionReq::parse(d["condition"].as_str()?).ok()?))).collect()).unwrap_or_default();
            Some(Release { version, link: t["link"].as_str()?.to_string(), hash: t["hash"].as_str().unwrap_or("").to_string(), size: t["size"].as_u64().unwrap_or(0), deps })
        })
        .collect();
    out.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(out)
}

/// Whether a game build passes a requirement like ">=491125" (space-separated parts all apply).
fn game_ok(req: &str, build: u64) -> bool {
    req.split_whitespace().all(|part| {
        let (op, num) = part.split_at(part.find(|c: char| c.is_ascii_digit()).unwrap_or(part.len()));
        let Ok(n) = num.parse::<u64>() else { return true };
        match op {
            ">=" | "^" | "~" => build >= n,
            ">" => build > n,
            "<=" => build <= n,
            "<" => build < n,
            "" | "=" => build == n,
            _ => true,
        }
    })
}

/// Works out what to download: the mod (newest release that fits) and every dependency that isn't
/// already installed in a matching version. Returns reference -> release.
fn plan(reference: &str, build: Option<u64>, have: &HashMap<String, Version>) -> Result<BTreeMap<String, Release>, String> {
    let mut todo = vec![(reference.to_string(), None::<VersionReq>)];
    let mut out: BTreeMap<String, Release> = BTreeMap::new();
    while let Some((r, req)) = todo.pop() {
        if out.get(&r).is_some_and(|rel| req.as_ref().is_none_or(|q| q.matches(&rel.version))) {
            continue;
        }
        if let (Some(q), Some(v)) = (&req, have.get(&r)) {
            if q.matches(v) {
                continue;
            }
        }
        let rel = releases(&r, build)?
            .into_iter()
            .find(|rel| req.as_ref().is_none_or(|q| q.matches(&rel.version)))
            .ok_or_else(|| format!("No version of {r} works with this game version{}", req.as_ref().map(|q| format!(" ({q})")).unwrap_or_default()))?;
        todo.extend(rel.deps.iter().map(|(d, q)| (d.clone(), Some(q.clone()))));
        out.insert(r, rel);
    }
    Ok(out)
}

/// Dev aid: what installing `reference` would download, without downloading.
pub fn dry_run(install: &Path, reference: &str) -> Result<Vec<String>, String> {
    let have: HashMap<String, Version> = installed(install).into_iter().filter_map(|m| Some((m.reference, Version::parse(&m.version).ok()?))).collect();
    Ok(plan(reference, game_build(install), &have)?.into_iter().map(|(r, rel)| format!("{r} {} ({} MB)", rel.version, rel.size >> 20)).collect())
}

/// Installs (or updates) a mod and what it needs. Returns the references that were installed.
pub fn install(install: &Path, reference: &str, cache: &Path, progress: impl Fn(Progress)) -> Result<Vec<String>, String> {
    let build = game_build(install);
    let have: HashMap<String, Version> = installed(install).into_iter().filter_map(|m| Some((m.reference, Version::parse(&m.version).ok()?))).collect();
    progress(Progress { reference: reference.into(), stage: "resolving", done: 0, total: 0 });
    let todo = plan(reference, build, &have)?;
    fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let mut done = Vec::new();
    for (r, rel) in &todo {
        check_name(r)?;
        let zip = cache.join(format!("{r}-{}.zip", rel.version));
        download(&format!("{API}{}", rel.link), &zip, &rel.hash, rel.size, |d, t| progress(Progress { reference: r.clone(), stage: "downloading", done: d, total: t }))?;
        progress(Progress { reference: r.clone(), stage: "installing", done: 0, total: 0 });
        unpack(&zip, &mods_dir(install), r)?;
        let _ = fs::remove_file(&zip);
        // A fresh install is always enabled; drop any disabled copy.
        let _ = fs::remove_dir_all(disabled_dir(install).join(r));
        done.push(r.clone());
    }
    progress(Progress { reference: reference.into(), stage: "done", done: 0, total: 0 });
    Ok(done)
}

fn download(url: &str, to: &Path, sha256: &str, size: u64, progress: impl Fn(u64, u64)) -> Result<(), String> {
    let resp = ureq::get(url).timeout(std::time::Duration::from_secs(600)).call().map_err(|e| format!("download failed: {e}"))?;
    let total = resp.header("Content-Length").and_then(|s| s.parse().ok()).unwrap_or(size);
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(to).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let (mut buf, mut got, mut last) = (vec![0u8; 1 << 16], 0u64, 0u64);
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        if got - last > 1 << 20 {
            last = got;
            progress(got, total);
        }
    }
    let hex: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if !sha256.is_empty() && !hex.eq_ignore_ascii_case(sha256) {
        let _ = fs::remove_file(to);
        return Err("Download was corrupted (checksum mismatch). Try again.".into());
    }
    Ok(())
}

/// Extracts a mod zip into Mods/<reference>, replacing the old copy only once extraction worked.
fn unpack(zip_path: &Path, mods: &Path, reference: &str) -> Result<(), String> {
    let staging = mods.join(format!(".{reference}.hotswap-new"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(fs::File::open(zip_path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name rejects paths that would escape the folder.
        let Some(rel) = f.enclosed_name() else { continue };
        let out = staging.join(rel);
        if f.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut w).map_err(|e| e.to_string())?;
    }
    let target = mods.join(reference);
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| format!("couldn't replace {reference}: {e}"))?;
    }
    fs::rename(&staging, &target).map_err(|e| e.to_string())
}

pub fn uninstall(install: &Path, reference: &str) -> Result<(), String> {
    check_name(reference)?;
    for dir in [mods_dir(install), disabled_dir(install)] {
        let p = dir.join(reference);
        if p.join(format!("{reference}.uplugin")).exists() {
            fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn set_enabled(install: &Path, reference: &str, on: bool) -> Result<(), String> {
    check_name(reference)?;
    let (from, to) = if on { (disabled_dir(install), mods_dir(install)) } else { (mods_dir(install), disabled_dir(install)) };
    let src = from.join(reference);
    if !src.join(format!("{reference}.uplugin")).exists() {
        return Ok(());
    }
    fs::create_dir_all(&to).map_err(|e| e.to_string())?;
    fs::rename(&src, to.join(reference)).map_err(|e| e.to_string())
}

/// Installed mods with a newer compatible release: reference -> newest version.
pub fn updates(install: &Path) -> Result<HashMap<String, String>, String> {
    let build = game_build(install);
    let mut out = HashMap::new();
    for m in installed(install) {
        let Ok(have) = Version::parse(&m.version) else { continue };
        if let Some(r) = releases(&m.reference, build).ok().and_then(|r| r.into_iter().next()) {
            if r.version > have {
                out.insert(m.reference, r.version.to_string());
            }
        }
    }
    Ok(out)
}

/// Mod references are plain identifiers; anything else could point outside the mods folder.
fn check_name(reference: &str) -> Result<(), String> {
    if reference.is_empty() || !reference.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err("Bad mod name".into());
    }
    Ok(())
}
