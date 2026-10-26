//! Finds Unreal Engine globals in a game's shipped .pdb files. Lookups are slow
//! (tens of MB of symbols), so callers cache the result per DLL.

use pdb::{FallibleIterator, SymbolData, PDB};
use std::{fs::File, path::Path};

/// Relative virtual address of the first symbol whose name contains `needle`.
/// Searches public symbols first, then each module's private ones (statics live there).
pub fn find_rva(pdb_path: &Path, needle: &str) -> Result<u32, String> {
    let file = File::open(pdb_path).map_err(|e| format!("{}: {e}", pdb_path.display()))?;
    let mut pdb = PDB::open(file).map_err(|e| e.to_string())?;
    let address_map = pdb.address_map().map_err(|e| e.to_string())?;
    let to_rva = |off: pdb::PdbInternalSectionOffset| off.to_rva(&address_map).map(|r| r.0);

    let globals = pdb.global_symbols().map_err(|e| e.to_string())?;
    let mut iter = globals.iter();
    while let Some(sym) = iter.next().map_err(|e| e.to_string())? {
        if let Ok(data) = sym.parse() {
            let hit = match data {
                SymbolData::Public(p) if p.name.to_string().contains(needle) => to_rva(p.offset),
                SymbolData::Data(d) if d.name.to_string().contains(needle) => to_rva(d.offset),
                _ => None,
            };
            if let Some(rva) = hit {
                return Ok(rva);
            }
        }
    }

    let dbi = pdb.debug_information().map_err(|e| e.to_string())?;
    let mut modules = dbi.modules().map_err(|e| e.to_string())?;
    while let Some(module) = modules.next().map_err(|e| e.to_string())? {
        let Some(info) = pdb.module_info(&module).map_err(|e| e.to_string())? else { continue };
        let mut syms = info.symbols().map_err(|e| e.to_string())?;
        while let Some(sym) = syms.next().map_err(|e| e.to_string())? {
            if let Ok(SymbolData::Data(d)) = sym.parse() {
                if d.name.to_string().contains(needle) {
                    if let Some(rva) = to_rva(d.offset) {
                        return Ok(rva);
                    }
                }
            }
        }
    }
    Err(format!("{needle} not found in {}", pdb_path.display()))
}
