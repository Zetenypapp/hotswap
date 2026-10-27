//! Reads what the Hotswap map shows from a running Satisfactory: resource nodes, collectibles,
//! buildings, and belts, pipes and power lines as lines. Everything comes from the game's memory;
//! no map data or artwork is bundled.

use crate::ue::objects::Ue;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct Node {
    x: f32,
    y: f32,
    z: f32,
    /// Resource descriptor, e.g. "OreIron", "LiquidOil".
    res: String,
    /// 0 impure, 1 normal, 2 pure.
    purity: u8,
    /// "node", "well" (fracking satellite), "core" (fracking core), "geyser" or "deposit".
    kind: &'static str,
}

#[derive(Serialize)]
pub struct Thing {
    x: f32,
    y: f32,
    z: f32,
    /// "slug1", "slug2", "slug3", "somersloop", "sphere" or "pod".
    kind: &'static str,
    /// Crash sites: already opened.
    done: bool,
}

#[derive(Serialize)]
pub struct Building {
    x: f32,
    y: f32,
    z: f32,
    yaw: f32,
    /// Class name without "Build_" and "_C", e.g. "SmelterMk1".
    class: String,
}

#[derive(Serialize)]
pub struct Line {
    /// "belt", "pipe", "rail", "power" or "hyper".
    kind: &'static str,
    pts: Vec<[f32; 3]>,
}

#[derive(Serialize, Default)]
pub struct World {
    nodes: Vec<Node>,
    things: Vec<Thing>,
    buildings: Vec<Building>,
    lines: Vec<Line>,
}

/// Property offsets, looked up once per class.
struct Reader<'a> {
    ue: &'a Ue,
    offs: HashMap<(u64, &'static str), Option<u64>>,
}

impl Reader<'_> {
    fn off(&mut self, obj: u64, name: &'static str) -> Option<u64> {
        let cls = self.ue.class_of(obj)?;
        let ue = self.ue;
        *self.offs.entry((cls, name)).or_insert_with(|| ue.prop(cls, name).map(|p| p.offset as u64))
    }

    /// An actor's location and yaw (degrees), from its root component.
    fn place(&mut self, actor: u64) -> Option<([f64; 3], f64)> {
        let root = self.ue.p.ptr(actor + self.off(actor, "RootComponent")?)?;
        let lo = self.off(root, "RelativeLocation")?;
        let ro = self.off(root, "RelativeRotation")?;
        let mut b = [0u8; 24];
        self.ue.p.read_into(root + lo, &mut b).then_some(())?;
        let v = |i: usize| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
        let loc = [v(0), v(1), v(2)];
        let yaw = self.ue.p.read::<f64>(root + ro + 8).unwrap_or(0.0);
        (loc.iter().all(|c| c.is_finite() && c.abs() < 1e7)).then_some((loc, yaw))
    }

    fn vec3(&self, addr: u64) -> Option<[f64; 3]> {
        let mut b = [0u8; 24];
        self.ue.p.read_into(addr, &mut b).then_some(())?;
        let p = [0, 1, 2].map(|i| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap()));
        p.iter().all(|c| c.is_finite() && c.abs() < 1e7).then_some(p)
    }

    fn flag(&mut self, obj: u64, name: &'static str) -> bool {
        let Some(cls) = self.ue.class_of(obj) else { return false };
        let Some(p) = self.ue.prop(cls, name) else { return false };
        self.ue.p.read::<u8>(obj + p.offset as u64 + p.bool_byte as u64).is_some_and(|b| b & p.bool_mask != 0)
    }
}

fn short(class: &str) -> String {
    class.trim_start_matches("Build_").trim_start_matches("Desc_").trim_end_matches("_C").to_string()
}

fn f(p: [f64; 3]) -> [f32; 3] {
    p.map(|c| c as f32)
}

/// Rotates a local point by `yaw` degrees and moves it to `at`.
fn to_world(at: [f64; 3], yaw: f64, l: [f64; 3]) -> [f64; 3] {
    let (s, c) = yaw.to_radians().sin_cos();
    [at[0] + c * l[0] - s * l[1], at[1] + s * l[0] + c * l[1], at[2] + l[2]]
}

pub fn scan(ue: &Ue) -> World {
    let mut r = Reader { ue, offs: HashMap::new() };
    let mut w = World::default();
    let mut names: HashMap<u64, String> = HashMap::new();
    let mut class_name = |o: u64| -> Option<String> {
        let c = ue.class_of(o)?;
        Some(names.entry(c).or_insert_with(|| ue.name_of(c).unwrap_or_default()).clone())
    };
    for o in ue.all_objects() {
        let Some(cls) = class_name(o) else { continue };
        let kind_node = match cls.as_str() {
            "BP_ResourceNode_C" => Some("node"),
            "BP_FrackingSatellite_C" => Some("well"),
            "BP_FrackingCore_C" => Some("core"),
            "BP_ResourceNodeGeyser_C" => Some("geyser"),
            "BP_ResourceDeposit_C" => Some("deposit"),
            _ => None,
        };
        let kind_thing = match cls.as_str() {
            "BP_Crystal_C" => Some("slug1"),
            "BP_Crystal_mk2_C" => Some("slug2"),
            "BP_Crystal_mk3_C" => Some("slug3"),
            "BP_WAT1_C" => Some("somersloop"),
            "BP_WAT2_C" => Some("sphere"),
            "BP_DropPod_C" => Some("pod"),
            _ => None,
        };
        let building = cls.starts_with("Build_");
        if kind_node.is_none() && kind_thing.is_none() && !building {
            continue;
        }
        if ue.name_of(o).is_some_and(|n| n.starts_with("Default__")) {
            continue;
        }
        // Unplaced objects (holograms, templates) sit at exactly the origin.
        let Some((at, yaw)) = r.place(o).filter(|(p, _)| p.iter().any(|c| *c != 0.0)) else { continue };
        let [x, y, z] = f(at);
        if let Some(kind) = kind_node {
            let res = r.off(o, "mResourceClass").and_then(|off| ue.p.ptr(o + off)).and_then(|c| ue.name_of(c)).map(|n| short(&n)).unwrap_or_default();
            let purity = r.off(o, "mPurity").and_then(|off| ue.p.read::<u8>(o + off)).unwrap_or(1).min(2);
            let res = if kind == "geyser" && res.is_empty() { "Geyser".into() } else { res };
            w.nodes.push(Node { x, y, z, res, purity, kind });
        } else if let Some(kind) = kind_thing {
            let done = kind == "pod" && (r.flag(o, "mHasBeenOpened") || r.flag(o, "mHasBeenLooted"));
            w.things.push(Thing { x, y, z, kind, done });
        } else {
            let lower = cls.to_ascii_lowercase();
            let line_kind = if lower.contains("conveyorbelt") { Some("belt") }
                else if lower.contains("pipeline") && !lower.contains("pump") && !lower.contains("support") && !lower.contains("junction") { Some("pipe") }
                else if lower.contains("railroadtrack") { Some("rail") }
                else if lower.contains("powerline") || lower.contains("wire") { Some("power") }
                else if lower.contains("pipehyper") && !lower.contains("support") && !lower.contains("start") { Some("hyper") }
                else { None };
            if let Some(kind) = line_kind {
                if let Some(pts) = line_points(&mut r, o, at, yaw) {
                    w.lines.push(Line { kind, pts });
                    continue;
                }
            }
            w.buildings.push(Building { x, y, z, yaw: yaw as f32, class: short(&cls) });
        }
    }
    w
}

/// Points along a belt, pipe or rail (its spline) or a power line (its two ends), in world space.
fn line_points(r: &mut Reader, o: u64, at: [f64; 3], yaw: f64) -> Option<Vec<[f32; 3]>> {
    let ue = r.ue;
    if let Some(off) = r.off(o, "mSplineData") {
        // TArray<FSplinePointData>: Location, ArriveTangent, LeaveTangent (doubles), relative to the actor.
        let data = ue.p.ptr(o + off)?;
        let n = ue.p.read::<i32>(o + off + 8)?.clamp(0, 400) as u64;
        let pts: Vec<[f32; 3]> = (0..n).filter_map(|i| r.vec3(data + i * 72)).map(|l| f(to_world(at, yaw, l))).collect();
        return (pts.len() >= 2).then_some(pts);
    }
    if let Some(off) = r.off(o, "mLocations") {
        let (a, b) = (r.vec3(o + off)?, r.vec3(o + off + 24)?);
        // Stored either in world space or relative to the actor; a world-space end sits near the actor.
        let d = |p: [f64; 3], q: [f64; 3]| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt();
        let world = d(a, at) < d(a, [0.0; 3]);
        let fix = |p| if world { f(p) } else { f(to_world(at, yaw, p)) };
        return Some(vec![fix(a), fix(b)]);
    }
    None
}
