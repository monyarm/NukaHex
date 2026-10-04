//! Fallout 4's terrain LOD textures (what the player sees of the land beyond the playable area), read
//! as one brightness per cell. They live in the game's texture archives (BA2, DX10 records) as
//! `textures\terrain\<world>\<world>.32.<x>.<y>.dds`: BC3 tiles of 512 px covering 32x32 cells.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const CELLS_PER_TILE: i32 = 32;

/// Mean brightness (0-255) of each cell's LOD texture, for the worldspace `world` (editor ID), from
/// the `Fallout4 - Textures*.ba2` archives in `data_dir`. None when no tile is found.
pub fn cell_brightness(data_dir: &Path, world: &str) -> Option<HashMap<(i32, i32), f32>> {
    let world = world.to_lowercase();
    let prefix = format!("textures\\terrain\\{0}\\{0}.32.", world);
    let mut archives: Vec<_> = std::fs::read_dir(data_dir).ok()?.flatten().map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("Fallout4 - Textures") && n.ends_with(".ba2")))
        .collect();
    archives.sort();
    let mut out = HashMap::new();
    for archive in archives {
        let Ok(tiles) = dx10_tiles(&archive, &prefix) else { continue };
        for ((tx, ty), (w, h, block)) in tiles {
            let px_per_cell = w as i32 / CELLS_PER_TILE;
            if px_per_cell < 4 || h != w { continue; }
            let blocks_per_row = w as usize / 4;
            let bpc = (px_per_cell / 4) as usize; // blocks per cell side
            for cy in 0..CELLS_PER_TILE {
                for cx in 0..CELLS_PER_TILE {
                    // Image rows run north to south; cell y grows north.
                    let (bx0, by0) = (cx as usize * bpc, cy as usize * bpc);
                    let mut sum = 0.0;
                    for by in by0..by0 + bpc { for bx in bx0..bx0 + bpc {
                        sum += bc3_block_brightness(&block[(by * blocks_per_row + bx) * 16..][..16]);
                    }}
                    out.insert((tx + cx, ty + CELLS_PER_TILE - 1 - cy), sum / (bpc * bpc) as f32);
                }
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Mean brightness of a BC3 block's 16 texels (its colour half: two RGB565 endpoints, 2-bit indices).
fn bc3_block_brightness(b: &[u8]) -> f32 {
    let c = |v: u16| [((v >> 11) & 31) as f32 * 255.0 / 31.0, ((v >> 5) & 63) as f32 * 255.0 / 63.0, (v & 31) as f32 * 255.0 / 31.0];
    let (c0, c1) = (c(u16::from_le_bytes([b[8], b[9]])), c(u16::from_le_bytes([b[10], b[11]])));
    let lum = |p: [f32; 3]| (p[0] + p[1] + p[2]) / 3.0;
    let palette = [lum(c0), lum(c1), (2.0 * lum(c0) + lum(c1)) / 3.0, (lum(c0) + 2.0 * lum(c1)) / 3.0];
    let idx = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
    (0..16).map(|i| palette[((idx >> (2 * i)) & 3) as usize]).sum::<f32>() / 16.0
}

/// The top mips of a DX10 archive's BC3 tiles whose path starts with `prefix`, by their tile's
/// south-west cell: (width, height, BC3 blocks).
fn dx10_tiles(path: &Path, prefix: &str) -> std::io::Result<Vec<((i32, i32), (u16, u16, Vec<u8>))>> {
    let mut f = File::open(path)?;
    let mut head = [0u8; 24];
    f.read_exact(&mut head)?;
    let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "not a DX10 BA2");
    if &head[0..4] != b"BTDX" || &head[8..12] != b"DX10" { return Err(bad()); }
    let version = u32::from_le_bytes(head[4..8].try_into().unwrap());
    let count = u32::from_le_bytes(head[12..16].try_into().unwrap()) as usize;
    let names_at = u64::from_le_bytes(head[16..24].try_into().unwrap());
    if version == 2 { f.seek(SeekFrom::Current(8))?; } else if version == 3 { f.seek(SeekFrom::Current(12))?; }
    // Records: 24-byte header (dimensions, DXGI format, chunk count), then 24 bytes per chunk.
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let mut r = [0u8; 24];
        f.read_exact(&mut r)?;
        let chunks = r[13] as usize;
        let (h, w, format) = (u16::from_le_bytes([r[16], r[17]]), u16::from_le_bytes([r[18], r[19]]), r[21]);
        let mut first = [0u8; 24];
        f.read_exact(&mut first)?;
        f.seek(SeekFrom::Current(24 * (chunks as i64 - 1)))?;
        let (offset, packed, size) = (u64::from_le_bytes(first[0..8].try_into().unwrap()),
            u32::from_le_bytes(first[8..12].try_into().unwrap()), u32::from_le_bytes(first[12..16].try_into().unwrap()));
        records.push((w, h, format, offset, packed, size));
    }
    f.seek(SeekFrom::Start(names_at))?;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let mut len = [0u8; 2];
        f.read_exact(&mut len)?;
        let mut name = vec![0u8; u16::from_le_bytes(len) as usize];
        f.read_exact(&mut name)?;
        names.push(String::from_utf8_lossy(&name).to_lowercase().replace('/', "\\"));
    }
    let mut out = Vec::new();
    for (name, &(w, h, format, offset, packed, size)) in names.iter().zip(&records) {
        // DXGI 77 = BC3; the diffuse tiles only (not `_msn` normals).
        let Some(rest) = name.strip_prefix(prefix).and_then(|r| r.strip_suffix(".dds")) else { continue };
        let mut xy = rest.split('.').map(|v| v.parse::<i32>());
        let (Some(Ok(x)), Some(Ok(y)), None) = (xy.next(), xy.next(), xy.next()) else { continue };
        if format != 77 { continue; }
        f.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0u8; if packed > 0 { packed } else { size } as usize];
        f.read_exact(&mut data)?;
        if packed > 0 {
            let mut raw = Vec::with_capacity(size as usize);
            flate2::read::ZlibDecoder::new(&data[..]).read_to_end(&mut raw)?;
            data = raw;
        }
        if data.len() >= w as usize * h as usize { out.push(((x, y), (w, h, data))); }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bc3_brightness() {
        // Endpoints white (0xFFFF) and black (0x0000); indices all 0 (white) or all 1 (black).
        let block = |idx: u32| { let mut b = [0u8; 16]; b[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes()); b[12..16].copy_from_slice(&idx.to_le_bytes()); b };
        assert!((bc3_block_brightness(&block(0)) - 255.0).abs() < 0.5);
        assert!(bc3_block_brightness(&block(0x5555_5555)).abs() < 0.5);
    }
}
