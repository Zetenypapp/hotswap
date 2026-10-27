//! Builds the Subnautica map background from the game's own terrain files in the user's install
//! (nothing is bundled): the compiled voxel octrees give the height of the seabed and islands
//! everywhere, and biomeMap.bin gives each spot's biome. The result is a shaded relief map,
//! colored by depth and tinted by biome, in the same files the other maps use.
//!
//! Terrain format (SNUnmanagedData/Build18): the world is 4096 x 3200 x 4096 one-meter voxels in
//! batches of 5 x 5 x 5 octrees of 32^3. A batch file is an i32 version (4), then per octree a
//! u16 node count and that many nodes of (u8 material, u8 distance, u16 first child); a node with
//! no children is a uniform cube, solid when its material isn't 0.

use crate::mapimage;
use std::path::Path;

pub const SIZE: usize = 4096;
const OCTREE: usize = 32;
const BATCH: usize = 5 * OCTREE;
/// Sea level in voxel units (world y = voxel y - SEA).
const SEA: f32 = 3040.0;

/// Height of the highest solid voxel in every 1 m column (voxel units, -1 = nothing).
fn heights(dir: &Path) -> Result<Vec<i16>, String> {
    let mut h = vec![-1i16; SIZE * SIZE];
    let rd = std::fs::read_dir(dir.join("CompiledOctreesCache")).map_err(|e| format!("terrain files not found: {e}"))?;
    let mut files: Vec<(usize, usize, usize, std::path::PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let idx = name.strip_prefix("compiled-batch-")?.strip_suffix(".optoctrees")?.to_string();
            let b: Vec<usize> = idx.split('-').filter_map(|v| v.parse().ok()).collect();
            let [bx, by, bz] = b[..] else { return None };
            Some((bx, by, bz, e.path()))
        })
        .collect();
    // Top layers first: once a batch's whole footprint has ground above it, it can't show and is skipped.
    files.sort_by(|a, b| b.1.cmp(&a.1));
    for (bx, by, bz, path) in files {
        let (x0, z0) = (bx * BATCH, bz * BATCH);
        let top = ((by + 1) * BATCH) as i16;
        let covered = (z0..(z0 + BATCH).min(SIZE)).all(|z| (x0..(x0 + BATCH).min(SIZE)).all(|x| h[z * SIZE + x] >= top));
        if covered {
            continue;
        }
        let Ok(data) = std::fs::read(path) else { continue };
        let mut pos = 4;
        for i in 0..125 {
            // Octree order inside a batch: z fastest, then y, then x.
            let (ox, oy, oz) = (i / 25, (i / 5) % 5, i % 5);
            let Some(n) = data.get(pos..pos + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) else { break };
            let nodes = &data[(pos + 2).min(data.len())..(pos + 2 + n * 4).min(data.len())];
            pos += 2 + n * 4;
            fill(nodes, 0, (x0 + ox * OCTREE, by * BATCH + oy * OCTREE, z0 + oz * OCTREE), OCTREE, &mut h);
        }
    }
    Ok(h)
}

/// Marks the top of every solid cube in node `i` (and its children) in the height map.
fn fill(nodes: &[u8], i: usize, (x, y, z): (usize, usize, usize), size: usize, h: &mut [i16]) {
    let Some(node) = nodes.get(i * 4..i * 4 + 4) else { return };
    let (material, child) = (node[0], u16::from_le_bytes([node[2], node[3]]) as usize);
    if child == 0 || size == 1 {
        if material == 0 {
            return;
        }
        let top = (y + size) as i16;
        for zz in z..(z + size).min(SIZE) {
            for xx in x..(x + size).min(SIZE) {
                let c = &mut h[zz * SIZE + xx];
                if *c < top {
                    *c = top;
                }
            }
        }
        return;
    }
    let half = size / 2;
    for c in 0..8 {
        let (dx, dy, dz) = ((c >> 2) & 1, (c >> 1) & 1, c & 1);
        fill(nodes, child + c, (x + dx * half, y + dy * half, z + dz * half), half, h);
    }
}

/// biomeMap.bin values, in biomes.csv order, and the tint each gets (None: no tint).
const BIOMES: [Option<[u8; 3]>; 19] = [
    Some([235, 215, 150]), // safeShallows
    Some([90, 165, 85]),   // kelpForest
    Some([200, 85, 70]),   // grassyPlateaus
    Some([120, 175, 200]), // underwaterIslands
    Some([175, 110, 200]), // mushroomForest
    Some([110, 125, 210]), // kooshZone
    Some([95, 115, 215]),  // grandReef
    Some([230, 240, 255]), // arctic
    Some([215, 95, 45]),   // inactiveLavaZone
    None,                  // unassigned
    Some([200, 170, 120]), // crashZone
    None,                  // void
    Some([165, 140, 200]), // sparseReef
    Some([215, 195, 145]), // dunes
    Some([150, 40, 65]),   // bloodKelp
    Some([150, 140, 155]), // mountains
    Some([175, 160, 120]), // seaTreaderPath
    Some([150, 40, 65]),   // bloodKelpTwo
    Some([135, 125, 115]), // CragField
];

fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Water color by depth in meters: light turquoise near the surface to near black far down.
fn depth_color(d: f32) -> [f32; 3] {
    const STOPS: [(f32, [f32; 3]); 8] = [
        (0.0, [125.0, 225.0, 215.0]), (30.0, [75.0, 195.0, 205.0]), (100.0, [40.0, 145.0, 185.0]), (250.0, [25.0, 100.0, 155.0]),
        (500.0, [20.0, 62.0, 118.0]), (900.0, [14.0, 36.0, 82.0]), (1400.0, [26.0, 20.0, 56.0]), (1700.0, [45.0, 16.0, 30.0]),
    ];
    for w in STOPS.windows(2) {
        if d <= w[1].0 {
            return lerp(w[0].1, w[1].1, ((d - w[0].0) / (w[1].0 - w[0].0)).clamp(0.0, 1.0));
        }
    }
    STOPS[7].1
}

/// Land above sea level: sand at the shore, then green, then rock.
fn land_color(up: f32) -> [f32; 3] {
    if up < 6.0 {
        lerp([225.0, 205.0, 150.0], [120.0, 160.0, 85.0], up / 6.0)
    } else {
        lerp([120.0, 160.0, 85.0], [150.0, 145.0, 135.0], ((up - 6.0) / 60.0).min(1.0))
    }
}

/// The finished map as RGB, SIZE x SIZE at 1 m per pixel, north up.
fn render(dir: &Path) -> Result<Vec<u8>, String> {
    let h = heights(dir)?;
    let biome = std::fs::read(dir.join("biomeMap.bin")).unwrap_or_default();
    let biome_at = |x: usize, z: usize| -> Option<[u8; 3]> {
        let v = *biome.get(2 + (z / 4) * 1024 + x / 4)? as usize;
        BIOMES.get(v).copied().flatten()
    };
    let at = |x: usize, z: usize| h[z.min(SIZE - 1) * SIZE + x.min(SIZE - 1)] as f32;
    let mut rgb = vec![0u8; SIZE * SIZE * 3];
    for z in 0..SIZE {
        for x in 0..SIZE {
            let v = at(x, z);
            let mut c = if v < 0.0 {
                [8.0, 16.0, 38.0]
            } else if v >= SEA {
                land_color(v - SEA)
            } else {
                depth_color(SEA - v)
            };
            if v >= 0.0 {
                if let Some(t) = biome_at(x, z) {
                    c = lerp(c, t.map(|v| v as f32), if v >= SEA { 0.08 } else { 0.14 });
                }
                // Relief, lit from the northwest.
                let (dx, dz) = (at(x + 1, z) - at(x.saturating_sub(1), z), at(x, z + 1) - at(x, z.saturating_sub(1)));
                let shade = (1.0 + (-dx + dz) * 0.09).clamp(0.45, 1.45);
                c = c.map(|v| v * shade);
            }
            let o = ((SIZE - 1 - z) * SIZE + x) * 3;
            for i in 0..3 {
                rgb[o + i] = c[i].clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok(rgb)
}

/// Writes the overview (2 m per pixel) and 2 x 2 full-size tiles (1 m per pixel) into `dir`,
/// unless they're already there for these game files. `data` is .../SNUnmanagedData/Build18.
pub fn ensure(data: &Path, dir: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(data.join("biomeMap.bin")).map_err(|e| format!("game files not found: {e}"))?;
    let stamp = format!("sn1-v1 {} {:?}", meta.len(), meta.modified().ok());
    let stamp_file = dir.join("stamp.txt");
    if std::fs::read_to_string(&stamp_file).is_ok_and(|s| s == stamp) && dir.join(mapimage::OVERVIEW).exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let rgb = render(data)?;
    let half = SIZE / 2;
    let mut overview = vec![0u8; half * half * 3];
    for y in 0..half {
        for x in 0..half {
            for i in 0..3 {
                let s: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(dx, dy)| rgb[((y * 2 + dy) * SIZE + x * 2 + dx) * 3 + i] as u32).sum();
                overview[(y * half + x) * 3 + i] = (s / 4) as u8;
            }
        }
    }
    mapimage::write_jpeg(&dir.join(mapimage::OVERVIEW), &overview, half)?;
    for (tx, ty) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        let mut part = vec![0u8; half * half * 3];
        for y in 0..half {
            let src = ((ty * half + y) * SIZE + tx * half) * 3;
            part[y * half * 3..(y + 1) * half * 3].copy_from_slice(&rgb[src..src + half * 3]);
        }
        mapimage::write_jpeg(&dir.join(mapimage::tile_file(tx, ty)), &part, half)?;
    }
    std::fs::write(stamp_file, stamp).map_err(|e| e.to_string())
}
