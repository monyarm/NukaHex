//! Fallout Tactics ZAR images (8-bit RLE with an embedded palette).
//!
//! Layout (reverse-engineered; see BrekeSFH/FTS `client/src/sprites/zar.ts`):
//! `<zar>` at 0, u32 width at 0x08, u32 height at 0x0C, 256 x (B, G, R, pad) palette at 0x15,
//! then u32 data size at 0x416 (standalone ZARs) or 0x415 (some embedded ones), then RLE runs.
//! Each run's control byte holds a count (high 6 bits) and a type (low 2 bits):
//! 0 = skip (transparent), 1 = palette indices, 2 = (index, alpha) pairs, 3 = alphas of colour 0.

const PALETTE_AT: usize = 0x15;
const SIZE_AT: [usize; 2] = [0x416, 0x415];

/// Walks the runs without writing pixels: (pixels covered, bytes consumed), or None if truncated.
fn scan(data: &[u8], total: usize) -> Option<(usize, usize)> {
    let (mut px, mut rd) = (0, 0);
    while rd < data.len() && px < total {
        let (n, t) = ((data[rd] >> 2) as usize, data[rd] & 3);
        rd += 1;
        if t != 0 {
            rd += n * if t == 2 { 2 } else { 1 };
            if rd > data.len() { return None; }
        }
        px = (px + n).min(total);
    }
    Some((px, rd))
}

/// Decodes a ZAR starting at `bytes[0]` to (width, height, RGB pixels); transparent pixels are black.
/// The size field's position varies, so the candidate whose runs exactly fill the image wins.
pub fn decode(bytes: &[u8]) -> Option<(usize, usize, Vec<[u8; 3]>)> {
    if !bytes.starts_with(b"<zar>") { return None; }
    let u32_at = |o: usize| Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?) as usize);
    let (w, h) = (u32_at(8)?, u32_at(12)?);
    let total = w.checked_mul(h).filter(|&t| t > 0 && t <= 64 << 20)?;
    let data = SIZE_AT.iter().find_map(|&at| {
        let size = u32_at(at)?;
        let d = bytes.get(at + 4..at + 4 + size)?;
        (scan(d, total) == Some((total, size))).then_some(d)
    })?;
    let pal: Vec<[u8; 3]> = bytes.get(PALETTE_AT..PALETTE_AT + 1024)?.chunks(4).map(|c| [c[2], c[1], c[0]]).collect();
    let mut out = vec![[0u8; 3]; total];
    let (mut px, mut rd) = (0, 0);
    while rd < data.len() && px < total {
        let (n, t) = ((data[rd] >> 2) as usize, data[rd] & 3);
        rd += 1;
        for _ in 0..n {
            if px >= total { break; }
            match t {
                1 => { out[px] = pal[data[rd] as usize]; rd += 1; }
                2 => { out[px] = pal[data[rd] as usize]; rd += 2; }
                3 => { rd += 1; }
                _ => {}
            }
            px += 1;
        }
    }
    Some((w, h, out))
}
