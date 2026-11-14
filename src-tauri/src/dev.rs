//! Dev aids, run with `hotswap-engine <--flag> ...` (they used to be `hotswap --flag`): print what
//! HotSwap sees in a running game, its library or its sensors, for development. Each one runs and
//! returns; nothing here runs in normal use.

#![allow(unused_imports)]
use crate::*;
use std::{collections::HashMap, fs, path::PathBuf};

pub fn run() {
    // Dev aid: `hotswap --gpu-sensors` lists the AMD driver's sensors.
    #[cfg(windows)]
    if std::env::args().any(|a| a == "--gpu-sensors") {
        println!("{:?}", stats::gpu_sensors());
        return;
    }
    // Dev aid: `hotswap --ets2-layout <exe>` reads ETS2's or ATS's save metadata from the exe.
    if let Some(i) = std::env::args().position(|a| a == "--ets2-layout") {
        let path = std::env::args().nth(i + 1).unwrap_or_default();
        println!("{}", ets2::describe(std::path::Path::new(&path)));
        return;
    }
    // Dev aid: `hotswap --engine-dump <engine> <exe name part> [Namespace/Class]` attaches the way the
    // engine mods do and lists the game's classes, or one class's fields with offsets and types.
    if let Some(i) = std::env::args().position(|a| a == "--engine-dump") {
        let a: Vec<String> = std::env::args().collect();
        let (engine, exe) = (a.get(i + 1).cloned().unwrap_or_default(), a.get(i + 2).cloned().unwrap_or_default().to_lowercase());
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let Some(pid) = sys.processes().values().find(|p| p.name().to_string_lossy().to_lowercase().contains(&exe)).map(|p| p.pid().as_u32()) else { return println!("not running") };
        let t = std::time::Instant::now();
        match engines::attach(&engine, pid) {
            Ok(mut r) => println!("{}\n({:?})", r.dump(a.get(i + 3).map(|s| s.as_str())), t.elapsed()),
            Err(e) => println!("attach failed: {e}"),
        }
        return;
    }
    // Dev aid: `hotswap --cdp <js> [spec json]` runs JavaScript in a web-tech game started by HotSwap;
    // with a spec map (`{"id": [spec, ...]}`), runs one tick of those mods turned on at value 1.
    if let Some(i) = std::env::args().position(|a| a == "--cdp") {
        let a: Vec<String> = std::env::args().collect();
        let mut s = match cdp::Session::attach() { Ok(s) => s, Err(e) => return println!("{e}") };
        println!("{:?}", s.eval(&a[i + 1]));
        if let Some(Ok(specs)) = a.get(i + 2).map(|j| serde_json::from_str::<HashMap<String, Vec<engines::Spec>>>(j)) {
            let on: HashMap<String, f64> = specs.keys().map(|k| (k.clone(), 1.0)).collect();
            let acts: Vec<String> = specs.iter().filter(|(_, l)| l.iter().any(|s| s.act)).map(|(k, _)| k.clone()).collect();
            println!("tick {:?}", s.tick(&specs, Some(&on), &acts));
            println!("{:?}", s.eval(&a[i + 1]));
        }
        return;
    }
    // Dev aid: `hotswap --catalog-verify <file>` checks a mod list against <file>.sig with the app's update key.
    if let Some(f) = std::env::args().skip_while(|a| a != "--catalog-verify").nth(1) {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap_or_default();
        let key = conf["plugins"]["updater"]["pubkey"].as_str().unwrap_or("");
        let (data, sig) = (fs::read(&f).unwrap_or_default(), fs::read_to_string(format!("{f}.sig")).unwrap_or_default());
        println!("{:?}", catalog::verify_with(key, &data, &sig));
        return;
    }
    // Dev aid: `hotswap --fthr [save_clip|toggle_recording|...]` prints FTHR Clips' status or sends it a command.
    if let Some(i) = std::env::args().position(|a| a == "--fthr") {
        fthr::test(std::env::args().nth(i + 1));
        return;
    }
    // Dev aid: `hotswap --fps <exe name>` prints that app's frame rate for ten seconds.
    if let Some(exe) = std::env::args().skip_while(|a| a != "--fps").nth(1) {
        println!("{}", fps::test(&exe));
        return;
    }
    // Dev aid: `hotswap --boost-display <exe name>` tests Game Boost's refresh rate switch on that app.
    if let Some(exe) = std::env::args().skip_while(|a| a != "--boost-display").nth(1) {
        println!("{}", boost::test_display(&exe));
        return;
    }
    // Dev aid: `hotswap --engine-test <engine> <exe name part> <path> <value> [type]` writes through a path.
    if let Some(i) = std::env::args().position(|a| a == "--engine-test") {
        let a: Vec<String> = std::env::args().collect();
        let exe = a.get(i + 2).cloned().unwrap_or_default().to_lowercase();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let Some(pid) = sys.processes().values().find(|p| p.name().to_string_lossy().to_lowercase().contains(&exe)).map(|p| p.pid().as_u32()) else { return println!("not running") };
        let v = a.get(i + 4).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        println!("{}", engines::test_path(&a[i + 1], pid, &a[i + 3], v, a.get(i + 5).map(|s| s.as_str())));
        return;
    }
    // Dev aid: `hotswap --engines` prints the engine of every installed game.
    if std::env::args().any(|a| a == "--engines") {
        for g in steam::scan().games {
            println!("{:<14} {} {}", g.engine.unwrap_or("-"), if g.single_player { "SP" } else { "  " }, g.name);
        }
        return;
    }
    // Dev aid: `hotswap --stats` prints two readings of the System tab's stats, a second apart.
    if std::env::args().any(|a| a == "--stats") {
        let st = stats::Stats::default();
        let _ = st.read(None);
        std::thread::sleep(std::time::Duration::from_secs(1));
        println!("{}", serde_json::to_string(&st.read(None)).unwrap_or_default());
        return;
    }
    // Dev aid: `hotswap --export-logs [--discord]` writes the logs zip and prints where.
    if std::env::args().any(|a| a == "--export-logs") {
        let small = std::env::args().any(|a| a == "--discord");
        println!("{}", logs::export(small, "exported from the command line").map(|p| p.display().to_string()).unwrap_or_else(|e| e));
        return;
    }
    // Dev aid: `hotswap --source <url>` prints what a Home panel would make of that address.
    if let Some(i) = std::env::args().position(|a| a == "--source") {
        let url = std::env::args().nth(i + 1).unwrap_or_default();
        println!("{}", sources::fetch(&url).map(|v| serde_json::to_string_pretty(&v).unwrap_or_default()).unwrap_or_else(|e| e));
        return;
    }
    // Dev aid: `tinker --scan` prints what the library scanner finds and exits.
    if std::env::args().any(|a| a == "--scan") {
        println!("{}", serde_json::to_string_pretty(&steam::scan()).unwrap_or_default());
        return;
    }
    // Dev aid: `tinker --appinfo <appid>` prints Steam's appinfo.vdf fields for one app.
    if let Some(id) = std::env::args().skip_while(|a| a != "--appinfo").nth(1).and_then(|s| s.parse::<u32>().ok()) {
        if let Some(root) = steam::steam_root() {
            let mut fields: Vec<_> = appinfo::app_fields(&root, &[id].into()).remove(&id).unwrap_or_default().into_iter().collect();
            fields.sort();
            for (k, v) in fields {
                println!("{k} = {v}");
            }
        }
        return;
    }
    // Dev aid: `tinker --home` fetches the live Home data once and prints it.
    if std::env::args().any(|a| a == "--home") {
        let ids: Vec<u32> = steam::scan().games.iter().filter(|g| g.kind == "game").map(|g| g.app_id).filter(|&id| id != 0).take(6).collect();
        println!("{}", serde_json::to_string_pretty(&home::fetch(&ids)).unwrap_or_default());
        return;
    }
    // Dev aid: `hotswap --pdb <file.pdb> <name>` prints a symbol's RVA.
    if let Some(i) = std::env::args().position(|a| a == "--pdb") {
        let a: Vec<String> = std::env::args().collect();
        let t = std::time::Instant::now();
        match ue::symbols::find_rva(std::path::Path::new(&a[i + 1]), &a[i + 2]) {
            Ok(rva) => println!("{} = 0x{rva:X} ({:?})", a[i + 2], t.elapsed()),
            Err(e) => println!("error: {e} ({:?})", t.elapsed()),
        }
        return;
    }
    // Dev aid: `hotswap --map-image <out dir>` builds the map background from the game files.
    if let Some(out) = std::env::args().skip_while(|a| a != "--map-image").nth(1) {
        let t = std::time::Instant::now();
        let install = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\Satisfactory");
        println!("{:?} in {:?}", mapimage::ensure(install, std::path::Path::new(&out)), t.elapsed());
        return;
    }
    // Dev aid: `hotswap --mods-plan <ModReference>` shows what installing a mod would download.
    if let Some(r) = std::env::args().skip_while(|a| a != "--mods-plan").nth(1) {
        let install = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\Satisfactory");
        println!("build {:?}", modloader::game_build(install));
        println!("{}", serde_json::to_string(&modloader::installed(install)).unwrap());
        println!("{:?}", modloader::dry_run(install, &r));
        println!("{:?}", modloader::search("smart", "downloads", 0).map(|v| v["count"].clone()));
        return;
    }
    // Dev aid: `hotswap --scan-attach <exe name part> [Class [filter]]` attaches without symbols and
    // lists a class's properties (or counts objects).
    if let Some(m) = std::env::args().skip_while(|a| a != "--scan-attach").nth(1) {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains(&m)).map(|p| p.pid().as_u32());
        let t = std::time::Instant::now();
        if let Some(pp) = pid.and_then(|p| ue::mem::Proc::open(p).ok()) {
            let (base, _) = pp.module(&m).unwrap_or_default();
            for s in ue::scan::sections(&pp, base) { println!("section {} 0x{:X} size 0x{:X}", s.0, s.1, s.2); }
            let (o, n) = ue::scan::find_tables(&pp, base);
            println!("object candidates {} name candidates {}", o.len(), n.len());
            for &c in o.iter().take(5) { println!("  obj cand 0x{c:X} num {:?}", pp.read::<i32>(c + 0x24)); }
            // Look for the "None" entry: a block whose first bytes are a 4-char header then "None".
            let mut shown = 0;
            for &c in n.iter() {
                let Some(b0) = pp.ptr(c + 0x10) else { continue };
                let mut h = [0u8; 16];
                pp.read_into(b0, &mut h);
                if h[2..6] == *b"None" || h[4..8] == *b"None" || h[..8].windows(4).any(|w| w == b"None") {
                    println!("  name cand 0x{c:X} block0 {:02X?}", h); shown += 1;
                    if shown > 5 { break; }
                }
            }
        }
        let u = match pid.map(|p| ue::objects::Ue::attach_scan(p, &m)) { Some(Ok(u)) => u, Some(Err(e)) => return println!("attach failed: {e}"), None => return println!("not running") };
        println!("attached in {:?}: {} objects", t.elapsed(), u.count());
        let args: Vec<String> = std::env::args().skip_while(|a| a != "--scan-attach").skip(2).collect();
        if let Some(cls) = args.first() {
            let filter = args.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
            if cls == "*" {
                // Class names matching the filter, with how many live objects each has.
                let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
                for o in u.all_objects() {
                    if let Some(c) = u.class_of(o).and_then(|c| u.name_of(c)) {
                        if c.to_lowercase().contains(&filter) { *counts.entry(c).or_default() += 1; }
                    }
                }
                for (k, n) in counts { println!("{n:6} {k}"); }
            } else if let Some(c) = u.find_class(cls) {
                for p in u.props(c).into_iter().filter(|p| p.name.to_lowercase().contains(&filter)) {
                    println!("{}.{} : {} @0x{:X}", p.owner, p.name, p.kind, p.offset);
                }
            } else {
                println!("class {cls} not found");
            }
        }
        return;
    }
    // Dev aid: `hotswap --sn2-peek` shows the Subnautica 2 player's attribute sets (base / current).
    if std::env::args().any(|a| a == "--sn2-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("Subnautica2-Win64")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach_scan(p, "Subnautica2-Win64").ok()) else { return println!("not attached") };
        let all = u.all_objects();
        println!("count {} live {}", u.count(), all.len());
        for &o in &all {
            if u.class_of(o).and_then(|c| u.name_of(c)).is_some_and(|n| n == "BP_SN2PlayerCharacter_C" || n == "BP_SN2PlayerController_C") {
                println!("  {} 0x{o:X} outer {:?}", u.name_of(o).unwrap_or_default(), u.outer_of(o).and_then(|x| u.name_of(x)));
                if let Some(c) = u.class_of(o) {
                    for f in ["Pawn", "AcknowledgedPawn", "Character", "PlayerState"] {
                        let v = u.prop(c, f).and_then(|pr| u.p.ptr(o + pr.offset as u64));
                        println!("    {f} = {v:X?} {:?}", v.and_then(|x| u.class_of(x)).and_then(|c| u.name_of(c)));
                    }
                }
            }
        }
        // The local player: what a live player controller possesses, plus its player state.
        let mut players = Vec::new();
        for c in u.find_by_classes(|n| n == "BP_SN2PlayerController_C") {
            let cls = u.class_of(c).unwrap();
            for f in ["Pawn", "PlayerState"] {
                if let Some(x) = u.prop(cls, f).and_then(|pr| u.p.ptr(c + pr.offset as u64)) { players.push(x); }
            }
        }
        for &p in &players {
            let cls = u.class_of(p).unwrap();
            let ctrl = u.prop(cls, "Controller").and_then(|o| u.p.ptr(p + o.offset as u64));
            println!("player 0x{p:X} {} {} controller={ctrl:X?}", u.name_of(cls).unwrap_or_default(), u.name_of(p).unwrap_or_default());
        }
        if let Some(&pawn) = players.first() {
            let pc = u.class_of(pawn).unwrap();
            if let Some(m) = u.prop(pc, "CharacterMovement").and_then(|o| u.p.ptr(pawn + o.offset as u64)) {
                let mc = u.class_of(m).unwrap();
                println!("movement 0x{m:X} {}", u.name_of(mc).unwrap_or_default());
                for f in ["MaxWalkSpeed", "MaxSwimSpeed", "MaxFlySpeed", "MaxCustomMovementSpeed", "MaxAcceleration", "ForwardAndStrafeAddWalkSpeed", "MovementMode", "CustomMovementMode"] {
                    if let Some(pr) = u.prop(mc, f) { println!("  {f} f32 {:?} u8 {:?}", u.p.read::<f32>(m + pr.offset as u64), u.p.read::<u8>(m + pr.offset as u64)); }
                }
            }
        }
        let sets = u.find_by_classes(|n| n.starts_with("UWE") && n.ends_with("AttributeSet"));
        for s in sets.into_iter().filter(|&s| u.outer_of(s).is_some_and(|o| players.contains(&o))) {
            let cls = u.class_of(s).unwrap();
            println!("== {} 0x{s:X} outer 0x{:X}", u.name_of(cls).unwrap_or_default(), u.outer_of(s).unwrap());
            for pr in u.props(cls).into_iter().filter(|p| p.kind == "StructProperty") {
                let a = s + pr.offset as u64;
                println!("  {} base {:?} current {:?}", pr.name, u.p.read::<f32>(a + 8), u.p.read::<f32>(a + 12));
            }
        }
        return;
    }
    // Dev aid: `hotswap --mono <process name part> <Class> [static field]` lists a Unity class's
    // fields through Mono, and the object a static field points to.
    if let Some(i) = std::env::args().position(|a| a == "--mono") {
        let a: Vec<String> = std::env::args().collect();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().to_lowercase().contains(&a[i + 1].to_lowercase())).map(|p| p.pid().as_u32());
        let mut m = match pid.map(mono::Mono::attach) { Some(Ok(m)) => m, Some(Err(e)) => return println!("attach failed: {e}"), None => return println!("not running") };
        println!("attached");
        let Some(cls_name) = a.get(i + 2) else { return };
        let (ns, name) = cls_name.rsplit_once('.').unwrap_or(("", cls_name));
        let Some(c) = m.class(ns, name) else { return println!("class {cls_name} not found") };
        if a.get(i + 3).map(|s| s.as_str()) == Some("enum") {
            for (n, v) in m.enum_values(c) { println!("  {n} = {v}"); }
            return;
        }
        for (n, off, ty) in m.fields_of(c) { println!("  {n} @0x{off:X} : {ty}"); }
        if let Some(f) = a.get(i + 3) {
            let addr = m.static_addr(c, f);
            println!("static {f} at {addr:X?} = {:X?}", addr.and_then(|x| m.p.ptr(x)));
        }
        return;
    }
    // Dev aid: `hotswap --scrap-test <survival folder copy>` adds and removes the bridge.
    if let Some(i) = std::env::args().position(|a| a == "--scrap-test") {
        let dir = PathBuf::from(std::env::args().nth(i + 1).unwrap_or_default());
        println!("install: {:?}", scrap::install(&dir));
        if std::env::args().nth(i + 2).as_deref() != Some("keep") {
            scrap::uninstall(&dir);
            println!("removed");
        }
        return;
    }
    // Dev aid: `hotswap --scrap-map <install> <out dir>` renders the map from a saved world.
    if let Some(i) = std::env::args().position(|a| a == "--scrap-map") {
        let a: Vec<String> = std::env::args().collect();
        println!("{:?}", scrap::ensure_map(&PathBuf::from(&a[i + 1]), &PathBuf::from(&a[i + 2])));
        return;
    }
    // Dev aid: `hotswap --ksp-peek` prints what the KSP trainer sees.
    if std::env::args().any(|a| a == "--ksp-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy() == "KSP_x64.exe").map(|p| p.pid().as_u32());
        match pid.map(mono::Mono::attach) { Some(Ok(m)) => ksp::peek(m), Some(Err(e)) => println!("attach failed: {e}"), None => println!("not running") }
        return;
    }
    // Dev aid: `hotswap --sn1-peek` prints Subnautica's live player speed values.
    if std::env::args().any(|a| a == "--sn1-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy() == "Subnautica.exe").map(|p| p.pid().as_u32());
        let Some(Ok(mut m)) = pid.map(mono::Mono::attach) else { return println!("not attached") };
        let pc_cls = m.class("", "Player").unwrap();
        let player = m.static_addr(pc_cls, "main").and_then(|a| m.p.ptr(a)).unwrap();
        let o = m.field(pc_cls, "<playerController>k__BackingField").unwrap() as u64;
        let pc = m.p.ptr(player + o).unwrap();
        let c = m.class("", "PlayerController").unwrap();
        for (n, off, ty) in m.fields_of(c) { if ty.contains("Single") || ty.contains("Motor") { println!("PC {n} = {:?} {:X?}", m.p.read::<f32>(pc + off as u64), m.p.ptr(pc + off as u64)); } }
        let (uwo, ao) = (m.field(c, "underWaterController").unwrap() as u64, m.field(c, "activeController").unwrap() as u64);
        let uw = m.p.ptr(pc + uwo).unwrap();
        let act = m.p.ptr(pc + ao);
        println!("underwater 0x{uw:X} active {act:X?}");
        let uc = m.class("", "UnderwaterMotor").unwrap();
        for (n, off, ty) in m.fields_of(uc) { if off != 0 && ty.contains("Single") { println!("UW {n} = {:?}", m.p.read::<f32>(uw + off as u64)); } }
        let mc = m.class("", "PlayerMotor").unwrap();
        for (n, off, ty) in m.fields_of(mc) { if ty.contains("Single") { println!("PM {n} = {:?}", m.p.read::<f32>(uw + off as u64)); } }
        return;
    }
    // Dev aid: `hotswap --sn1-map <out.jpg>` renders Subnautica's terrain heights.
    if let Some(out) = std::env::args().skip_while(|a| a != "--sn1-map").nth(1) {
        let t = std::time::Instant::now();
        let dir = std::path::Path::new(r"E:\SteamLibrary\steamapps\common\Subnautica\Subnautica_Data\StreamingAssets\SNUnmanagedData\Build18");
        if out == "stats" { return println!("{}", sn1map::material_stats(dir).unwrap_or_else(|e| e)); }
        println!("{:?} in {:?}", sn1map::ensure(dir, std::path::Path::new(&out), sn1map::Style::Satellite), t.elapsed());
        return;
    }
    // Dev aid: `hotswap --repo-peek` prints the R.E.P.O. player's and enemies' positions.
    if std::env::args().any(|a| a == "--repo-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy() == "REPO.exe").map(|p| p.pid().as_u32());
        let Some(Ok(mut m)) = pid.map(mono::Mono::attach) else { return println!("not attached") };
        let pa = m.class("", "PlayerAvatar").unwrap();
        let avatar = m.static_addr(pa, "instance").and_then(|a| m.p.ptr(a)).unwrap();
        let (to, co) = (m.field(pa, "playerTransform").unwrap() as u64, m.field(pa, "clientPositionCurrent").unwrap() as u64);
        let t = m.p.ptr(avatar + to).unwrap();
        let cp: Vec<f32> = (0..3).map(|i| m.p.read::<f32>(avatar + co + i * 4).unwrap()).collect();
        println!("player transform {:?} clientPositionCurrent {cp:?}", m.transform_pos(t));
        let ed = m.class("", "EnemyDirector").unwrap();
        let dir = m.static_addr(ed, "instance").and_then(|a| m.p.ptr(a)).unwrap();
        let lo = m.field(ed, "enemiesSpawned").unwrap() as u64;
        let list = m.p.ptr(dir + lo).unwrap();
        let (items, n) = (m.p.ptr(list + 0x10).unwrap(), m.p.read::<i32>(list + 0x18).unwrap());
        let ep = m.class("", "EnemyParent").unwrap();
        let (eo, no, so) = (m.field(ep, "Enemy").unwrap() as u64, m.field(ep, "enemyName").unwrap() as u64, m.field(ep, "Spawned").unwrap() as u64);
        let en = m.class("", "Enemy").unwrap();
        let ct = m.field(en, "CenterTransform").unwrap() as u64;
        for i in 0..n as u64 {
            let Some(par) = m.p.ptr(items + 0x20 + i * 8) else { continue };
            let name = m.p.ptr(par + no).map(|s| { let len = m.p.read::<i32>(s + 0x10).unwrap_or(0).clamp(0, 64) as usize; let mut b = vec![0u8; len * 2]; m.p.read_into(s + 0x14, &mut b); String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()) });
            let spawned = m.p.read::<u8>(par + so);
            let pos = m.p.ptr(par + eo).and_then(|e| m.p.ptr(e + ct)).and_then(|t| m.transform_pos(t));
            println!("  {name:?} spawned={spawned:?} at {pos:?}");
        }
        return;
    }
    // Dev aid: `hotswap --world-scan` prints what the map would draw.
    if std::env::args().any(|a| a == "--world-scan") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        let t = std::time::Instant::now();
        let w = serde_json::to_value(world::scan(&u)).unwrap();
        if let Some(out) = std::env::args().skip_while(|a| a != "--world-scan").nth(1) {
            let _ = fs::write(out, w.to_string());
        }
        println!("scan {:?}", t.elapsed());
        for k in ["nodes", "things", "buildings", "lines"] {
            let a = w[k].as_array().unwrap();
            println!("{k}: {} e.g. {}", a.len(), a.iter().take(3).map(|v| v.to_string()).collect::<Vec<_>>().join(" | "));
        }
        let mut res: std::collections::BTreeMap<String, usize> = Default::default();
        for n in w["nodes"].as_array().unwrap() { *res.entry(format!("{} {} p{}", n["kind"], n["res"], n["purity"])).or_default() += 1; }
        println!("{res:?}");
        return;
    }
    // Dev aid: `hotswap --world-peek` counts map-worthy actor classes and shows key properties.
    if std::env::args().any(|a| a == "--world-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        let t = std::time::Instant::now();
        let mut counts: std::collections::BTreeMap<String, (usize, u64)> = Default::default();
        for o in u.find_by_classes(|n| n.starts_with("Build_") || n.contains("Resource") || n.contains("Fracking") || n.contains("DropPod") || n.contains("Crystal") || n.contains("WAT") || n.contains("Lightweight")) {
            let c = u.class_of(o).unwrap();
            let e = counts.entry(u.name_of(c).unwrap_or_default()).or_insert((0, o));
            e.0 += 1;
        }
        println!("scan {:?}", t.elapsed());
        for (k, (n, o)) in &counts { println!("{n:6} {k}  e.g. 0x{o:X}"); }
        for want in ["Build_ConveyorBeltMk1_C", "BP_ResourceNode_C", "Build_PowerLine_C", "FGLightweightBuildableSubsystem", "BP_DropPod_C"] {
            if let Some((_, o)) = counts.get(want) {
                let c = u.class_of(*o).unwrap();
                let names: Vec<String> = u.props(c).into_iter().filter(|p| !p.owner.starts_with("Actor") && !p.owner.starts_with("Object")).map(|p| format!("{}:{}@{:X}", p.name, p.kind, p.offset)).collect();
                println!("== {want}: {}", names.join(", "));
            }
        }
        return;
    }
    // Dev aid: `hotswap --tp-peek` finds every copy of the player's location in its components.
    if std::env::args().any(|a| a == "--tp-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        for p in u.find_by_class("Char_Player_C") {
            let cls = u.class_of(p).unwrap();
            if u.prop(cls, "Controller").and_then(|o| u.p.ptr(p + o.offset as u64)).is_none() { continue }
            let root = u.prop(cls, "RootComponent").and_then(|o| u.p.ptr(p + o.offset as u64)).unwrap();
            let rc = u.class_of(root).unwrap();
            let rel = u.prop(rc, "RelativeLocation").unwrap().offset as u64;
            let here: Vec<f64> = (0..3).map(|i| u.p.read::<f64>(root + rel + i * 8).unwrap()).collect();
            println!("player 0x{p:X} root 0x{root:X} {} RelativeLocation +0x{rel:X} = {here:?}", u.name_of(rc).unwrap_or_default());
            let m = u.prop(cls, "CharacterMovement").and_then(|o| u.p.ptr(p + o.offset as u64)).unwrap();
            for (label, obj) in [("root", root), ("movement", m), ("player", p)] {
                let mut buf = vec![0u8; 0x1000];
                u.p.read_into(obj, &mut buf);
                for o in (0..0x1000 - 24).step_by(8) {
                    let d = |k: usize| f64::from_le_bytes(buf[o + k * 8..o + k * 8 + 8].try_into().unwrap());
                    if (d(0) - here[0]).abs() < 2000.0 && (d(1) - here[1]).abs() < 2000.0 && (d(2) - here[2]).abs() < 2000.0 {
                        println!("  {label} +0x{o:X}: {:?}", [d(0), d(1), d(2)]);
                    }
                }
                for pr in u.props(u.class_of(obj).unwrap()).iter().filter(|x| x.name.contains("Location") || x.name.contains("Transform")) {
                    println!("    prop {label}.{} +0x{:X} {}", pr.name, pr.offset, pr.kind);
                }
            }
        }
        return;
    }
    // Dev aid: `hotswap --trainer-peek` prints the player's live movement values without writing.
    if std::env::args().any(|a| a == "--trainer-peek") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        for p in u.find_by_class("Char_Player_C") {
            let cls = u.class_of(p).unwrap();
            let ctrl = u.prop(cls, "Controller").and_then(|o| u.p.ptr(p + o.offset as u64));
            let Some(m) = u.prop(cls, "CharacterMovement").and_then(|o| u.p.ptr(p + o.offset as u64)) else { continue };
            let mc = u.class_of(m).unwrap();
            let read = |n: &str| u.prop(mc, n).and_then(|o| u.p.read::<f32>(m + o.offset as u64));
            println!("player 0x{p:X} ctrl={ctrl:X?} walk={:?} sprint={:?} jump={:?} gravity={:?}", read("MaxWalkSpeed"), read("mMaxSprintSpeed"), read("JumpZVelocity"), read("GravityScale"));
        }
        for c in ["BP_GameState_C", "FGGameState"] {
            for gs in u.find_by_class(c) {
                let cls = u.class_of(gs).unwrap();
                let flags: Vec<String> = ["mCheatNoCost", "mCheatNoPower", "mCheatNoFuel", "mCheatTurboProductionMode", "mCheatTurboBuildMode"]
                    .iter().map(|n| format!("{n}={:?}", u.prop(cls, n).and_then(|o| u.p.read::<u8>(gs + o.offset as u64)))).collect();
                println!("{c} 0x{gs:X}: {}", flags.join(" "));
            }
        }
        return;
    }
    // Dev aid: `hotswap --call-test` adds 10 iron plates to the player's inventory via AddStack.
    if std::env::args().any(|a| a == "--call-test") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        println!("ProcessEvent = 0x{:X}", u.process_event);
        let Some(f) = u.func("FGInventoryComponent", "AddStack") else { println!("AddStack not found"); return };
        println!("AddStack 0x{:X} params {:?}", f.addr, f.params);
        let plate = u.find_class("Desc_IronPlate_C").unwrap_or(0);
        let player = u.find_by_class("Char_Player_C").into_iter().find(|&p| {
            u.class_of(p).and_then(|c| u.prop(c, "Controller")).and_then(|o| u.p.ptr(p + o.offset as u64)).is_some()
        });
        let Some(player) = player else { println!("no player"); return };
        let inv = u.class_of(player).and_then(|c| u.prop(c, "mInventory")).and_then(|o| u.p.ptr(player + o.offset as u64)).unwrap_or(0);
        let off = |n: &str| f.params.iter().find(|p| p.0 == n).map(|p| p.1);
        let mut params = vec![0u8; f.size as usize];
        let st = off("stack").or_else(|| off("Stack")).unwrap_or(0) as usize;
        params[st + 8..st + 16].copy_from_slice(&plate.to_le_bytes());
        params[st + 0x28..st + 0x2C].copy_from_slice(&10i32.to_le_bytes());
        match u.call(u.process_event, inv, &f, &params) {
            Ok(out) => {
                let ret = off("ReturnValue").map(|o| i32::from_le_bytes(out[o as usize..o as usize + 4].try_into().unwrap()));
                println!("called OK, added {ret:?}");
            }
            Err(e) => println!("call failed: {e}"),
        }
        return;
    }
    // Dev aid: `hotswap --machine-peek <Build_X_C>` shows a machine's recipe and input slots.
    if let Some(i) = std::env::args().position(|a| a == "--machine-peek") {
        let cls_name = std::env::args().nth(i + 1).unwrap_or_default();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|p| ue::objects::Ue::attach(p).ok()) else { println!("not attached"); return };
        let recipe_filter = std::env::args().nth(i + 2).unwrap_or_default();
        let all = u.find_by_class(&cls_name);
        let pick: Vec<u64> = all.into_iter().filter(|&m| recipe_filter.is_empty() || u.class_of(m).and_then(|c| u.prop(c, "mCurrentRecipe")).and_then(|p| u.p.ptr(m + p.offset as u64)).and_then(|r| u.name_of(r)).is_some_and(|n| n.contains(&recipe_filter))).take(2).collect();
        for m in pick {
            let cls = u.class_of(m).unwrap();
            let get = |n: &str| u.prop(cls, n).map(|p| m + p.offset as u64);
            let recipe = get("mCachedRecipe").and_then(|a| u.p.ptr(a));
            let cur = get("mCurrentRecipe").and_then(|a| u.p.ptr(a)).and_then(|c| u.name_of(c));
            println!("machine 0x{m:X} current={cur:?} cached={recipe:X?} ({:?})", recipe.and_then(|r| u.name_of(r)));
            let f = |n: &str| get(n).and_then(|a| u.p.read::<f32>(a));
            let b = |n: &str| get(n).and_then(|a| u.p.read::<u8>(a));
            println!("  progress={:?} potential cur={:?} pending={:?} max={:?} paused={:?}", f("mCurrentManufacturingProgress"), f("mCurrentPotential"), f("mPendingPotential"), f("mMaxPotential"), b("mIsProductionPaused"));
            if let Some(r) = recipe {
                let rc = u.class_of(r).unwrap();
                if let Some(p) = u.prop(rc, "mIngredients") {
                    let (data, n) = (u.p.ptr(r + p.offset as u64).unwrap_or(0), u.p.read::<i32>(r + p.offset as u64 + 8).unwrap_or(0));
                    for k in 0..n.clamp(0, 8) as u64 {
                        let item = u.p.read::<u64>(data + k * 16).unwrap_or(0);
                        println!("  needs {} x{}", u.name_of(item).unwrap_or_default(), u.p.read::<i32>(data + k * 16 + 8).unwrap_or(0));
                    }
                }
            }
            for (label, prop) in [("out", "mOutputInventory")] {
                if let Some(inv) = get(prop).and_then(|a| u.p.ptr(a)) {
                    let ic = u.class_of(inv).unwrap();
                    let arr = inv + u.prop(ic, "mInventoryStacks").unwrap().offset as u64;
                    let (data, n) = (u.p.ptr(arr).unwrap_or(0), u.p.read::<i32>(arr + 8).unwrap_or(0));
                    for k in 0..n.clamp(0, 8) as u64 {
                        let s = data + k * 0x30;
                        let item = u.p.read::<u64>(s + 8).unwrap_or(0);
                        println!("  {label} {k}: {} x{}", u.name_of(item).unwrap_or("empty".into()), u.p.read::<i32>(s + 0x28).unwrap_or(0));
                    }
                }
            }
            if let Some(inv) = get("mInputInventory").and_then(|a| u.p.ptr(a)) {
                let ic = u.class_of(inv).unwrap();
                let arr = inv + u.prop(ic, "mInventoryStacks").unwrap().offset as u64;
                let (data, n) = (u.p.ptr(arr).unwrap_or(0), u.p.read::<i32>(arr + 8).unwrap_or(0));
                for k in 0..n.clamp(0, 8) as u64 {
                    let s = data + k * 0x30;
                    let item = u.p.read::<u64>(s + 8).unwrap_or(0);
                    println!("  slot {k}: {} x{}", u.name_of(item).unwrap_or("empty".into()), u.p.read::<i32>(s + 0x28).unwrap_or(0));
                }
            }
        }
        return;
    }
    if std::env::args().any(|a| a == "--trainer-test") {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        match sys.processes().values().find(|p| p.name().to_string_lossy().contains("FactoryGameSteam-Win64-Shipping")) {
            Some(p) => trainer::debug(p.pid().as_u32()),
            None => println!("Satisfactory isn't running"),
        }
        return;
    }
    // Dev aid: `hotswap --ue-props <ClassName> [filter]` lists a class's properties in the running game.
    if let Some(i) = std::env::args().position(|a| a == "--ue-props") {
        let a: Vec<String> = std::env::args().collect();
        let filter = a.get(i + 2).map(|s| s.to_lowercase()).unwrap_or_default();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let pid = sys.processes().values().find(|p| p.name().to_string_lossy().contains("Win64-Shipping")).map(|p| p.pid().as_u32());
        let Some(u) = pid.and_then(|pid| ue::objects::Ue::attach(pid).ok()) else { println!("no game"); return };
        let Some(cls) = u.find_class(&a[i + 1]) else { println!("class not found"); return };
        for p in u.props(cls).iter().filter(|p| p.name.to_lowercase().contains(&filter)) {
            println!("  {:<40} {:<18} +0x{:<5X} {}", p.name, p.kind, p.offset, p.owner);
        }
        return;
    }
    // Dev aid: `hotswap --ue-classes <word> [word...]` lists the running Unreal game's classes (C++ and
    // Blueprint) whose names contain one of the words, to find what a new game's mods should change.
    if let Some(i) = std::env::args().position(|a| a == "--ue-classes") {
        let words: Vec<String> = std::env::args().skip(i + 1).map(|w| w.to_lowercase()).collect();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let Some(p) = sys.processes().values().find(|p| p.name().to_string_lossy().contains("Win64-Shipping")) else { println!("no game"); return };
        let stem = p.name().to_string_lossy().trim_end_matches(".exe").to_string();
        // As the engine mods attach: with a search for the object tables if the game's symbols aren't known.
        let u = match ue::objects::Ue::attach_scan(p.pid().as_u32(), &stem) {
            Ok(u) => u,
            Err(e) => return println!("attach failed: {e}"),
        };
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..u.count() {
            let Some(o) = u.object(i) else { continue };
            let kind = u.class_of(o).and_then(|c| u.name_of(c)).unwrap_or_default();
            if !kind.ends_with("Class") || kind.contains("Function") {
                continue;
            }
            let name = u.name_of(o).unwrap_or_default();
            let lower = name.to_lowercase();
            if words.iter().any(|w| lower.contains(w)) && !name.starts_with("Default__") {
                seen.insert(format!("{name} ({kind})"));
            }
        }
        for s in seen.iter().take(300) {
            println!("{s}");
        }
        return;
    }
    // Dev aid: `hotswap --fs-install <787860|1248130>` puts the HotSwap mod in Farming Simulator 19's or
    // 22's mods folder, as opening the game's page does.
    if let Some(i) = std::env::args().position(|a| a == "--fs-install") {
        let game = std::env::args().nth(i + 1).unwrap_or_default();
        println!("{:?}", farming::install(&game));
        return;
    }
    // Dev aid: `hotswap --ue-raw <ClassName>` prints a class's first words, each with the name of
    // the object it points to, to work out a game's UStruct and FField layout.
    if let Some(i) = std::env::args().position(|a| a == "--ue-raw") {
        let name = std::env::args().nth(i + 1).unwrap_or_default();
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let Some(p) = sys.processes().values().find(|p| p.name().to_string_lossy().contains("Win64-Shipping")) else { println!("no game"); return };
        let stem = p.name().to_string_lossy().trim_end_matches(".exe").to_string();
        let u = match ue::objects::Ue::attach_scan(p.pid().as_u32(), &stem) { Ok(u) => u, Err(e) => return println!("attach failed: {e}") };
        let Some(c) = u.find_class(&name) else { return println!("no class {name}") };
        println!("{name} at 0x{c:X}");
        for off in (0..0x80u64).step_by(8) {
            let v = u.p.read::<u64>(c + off).unwrap_or(0);
            // What it points to: an object's name, or (for a property list) the words of an FField.
            let obj = u.name_of(v).unwrap_or_default();
            let field: Vec<String> = (0..6u64).map(|k| format!("{:X}", u.p.read::<u64>(v + k * 8).unwrap_or(0))).collect();
            println!("  +0x{off:02X} {v:016X} {obj:<30} [{}]", field.join(" "));
        }
        return;
    }
    // Dev aid: `hotswap --ue-dump [ClassName]` attaches to a running Unreal game and lists objects.
    if let Some(i) = std::env::args().position(|a| a == "--ue-dump") {
        let class = std::env::args().nth(i + 1);
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let Some(pid) = sys.processes().values().find(|p| p.name().to_string_lossy().contains("Win64-Shipping")).map(|p| p.pid().as_u32()) else {
            println!("No *-Win64-Shipping.exe game running");
            return;
        };
        match ue::objects::Ue::attach(pid) {
            Err(e) => println!("attach failed: {e}"),
            Ok(u) => {
                println!("attached to {pid}: {} objects", u.count());
                for i in 0..12 {
                    if let Some(o) = u.object(i) {
                        let cls = u.class_of(o).and_then(|c| u.name_of(c)).unwrap_or_default();
                        println!("  [{i}] {} : {cls}", u.name_of(o).unwrap_or_default());
                    }
                }
                if let Some(c) = class {
                    let found = u.find_by_class(&c);
                    println!("{c}: {} found", found.len());
                    for o in found.iter().take(5) {
                        let outer = u.outer_of(*o).and_then(|x| u.name_of(x)).unwrap_or_default();
                        println!("  0x{o:X} {} (in {outer})", u.name_of(*o).unwrap_or_default());
                    }
                }
            }
        }
        return;
    }
    println!("Unknown dev flag");
}
