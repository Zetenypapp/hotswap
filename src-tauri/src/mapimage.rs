//! Builds the map background from the game's own map texture in the user's install: four
//! 4096x4096 BC1 tiles (Map_X-Y) read with the IoStore reader, stitched and saved as one JPEG in
//! the app data folder. Nothing from the game is bundled or uploaded.

use crate::iostore::Container;
use std::path::Path;

const TILE: usize = 4096;
const TILE_DIR: &str = "/interface/ui/assets/maptest/slicedmap/";

/// Files written to the map folder: a small overview of the whole map, and each tile at full size.
pub const OVERVIEW: &str = "overview.jpg";
pub fn tile_file(tx: usize, ty: usize) -> String {
    format!("tile_{tx}_{ty}.jpg")
}

/// Builds the map images in `dir` from the game at `install`, unless they're already there for
/// this version of the game's files.
pub fn ensure(install: &Path, dir: &Path) -> Result<(), String> {
    let utoc = install.join(r"FactoryGame\Content\Paks\FactoryGame-Windows.utoc");
    let meta = std::fs::metadata(&utoc).map_err(|e| format!("game files not found: {e}"))?;
    let stamp = format!("{} {:?}", meta.len(), meta.modified().ok());
    let stamp_file = dir.join("stamp.txt");
    if std::fs::read_to_string(&stamp_file).is_ok_and(|s| s == stamp) && dir.join(OVERVIEW).exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let c = Container::open(&utoc)?;
    const OV: usize = 2048; // overview size; each tile covers a quarter
    let (half, step) = (OV / 2, TILE / (OV / 2));
    let mut overview = vec![0u8; OV * OV * 3];
    for (tx, ty) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        let name = format!("{TILE_DIR}map_{tx}-{ty}.ubulk");
        let entry = c.files.iter().find(|(p, _)| p.ends_with(&name)).map(|(_, &i)| i).ok_or(format!("map tile {tx}-{ty} not found"))?;
        let bulk = c.read(entry)?;
        // The bulk data holds the mips largest first; the top one is TILE x TILE BC1 (half a byte per pixel).
        let top = bulk.get(..TILE * TILE / 2).ok_or("map tile is smaller than expected")?;
        let mut px = vec![0u32; TILE * TILE];
        texture2ddecoder::decode_bc1(top, TILE, TILE, &mut px)?;
        let rgb: Vec<u8> = px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        write_jpeg(&dir.join(tile_file(tx, ty)), &rgb, TILE)?;
        // Box-filter into the overview: tile x goes across, tile y goes down.
        for oy in 0..half {
            for ox in 0..half {
                let mut sum = [0u32; 3];
                for sy in 0..step {
                    for sx in 0..step {
                        let i = ((oy * step + sy) * TILE + ox * step + sx) * 3;
                        (0..3).for_each(|k| sum[k] += rgb[i + k] as u32);
                    }
                }
                let o = ((ty * half + oy) * OV + tx * half + ox) * 3;
                (0..3).for_each(|k| overview[o + k] = (sum[k] / (step * step) as u32) as u8);
            }
        }
    }
    write_jpeg(&dir.join(OVERVIEW), &overview, OV)?;
    std::fs::write(stamp_file, stamp).map_err(|e| e.to_string())
}

fn write_jpeg(path: &Path, rgb: &[u8], size: usize) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let enc = jpeg_encoder::Encoder::new_file(&tmp, 88).map_err(|e| e.to_string())?;
    enc.encode(rgb, size as u16, size as u16, jpeg_encoder::ColorType::Rgb).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}
