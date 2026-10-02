//! Fallout 1/2 engine image formats: FRM (8-bit paletted frames) and PAL (palettes).

/// First frame of an FRM: (width, height, palette indices in row order).
/// FRM header is 62 bytes, then per frame: u16 width, u16 height, u32 size, i16 x, i16 y (big-endian).
pub fn decode_frm(bytes: &[u8]) -> Option<(usize, usize, &[u8])> {
    let be16 = |o: usize| Some(u16::from_be_bytes(bytes.get(o..o + 2)?.try_into().ok()?) as usize);
    let (w, h) = (be16(62)?, be16(64)?);
    Some((w, h, bytes.get(74..74 + w * h)?))
}

/// 256 RGB colours from a PAL file (6-bit components, scaled to 8-bit).
pub fn palette(bytes: &[u8]) -> Option<Vec<[u8; 3]>> {
    let raw = bytes.get(..768)?;
    Some(raw.chunks(3).map(|c| [c[0].saturating_mul(4), c[1].saturating_mul(4), c[2].saturating_mul(4)]).collect())
}
