//! Van Buren (Black Isle's cancelled Fallout 3), from the 2003 tech demo.
//!
//! The demo has no world-map location table; everything comes from the world-map art
//! (`F3_WorldMap`, a TGA in `Interface.grp`):
//! - location markers: 46x46 green rings; names read from their drawn labels (tesseract OCR,
//!   snapped to the known world-map location names)
//! - anchors: the drawn state borders, whose crossings are known real coordinates
//! - terrain: teal water, high-relief mountains, everything else sand

use crate::extractors::Extractor;
use crate::schema::{IconType, RawControlPoint, RawLocation, RawMapData, RawTile};
use image::RgbImage;
use log::{info, warn};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DEMO_URL: &str = "https://vb.kran.gg/F3_Demo.rar";
const CACHE_RAR: &str = ".cache/van_buren/F3_Demo.rar";
/// Terrain block size in map pixels (~2.8 mi).
const BLOCK_PX: usize = 10;
/// Fraction of the map (by local relief) treated as mountains.
// ponytail: fixed quantile; calibrated by eye on the demo map, revisit if mountains look too thin/thick.
const MOUNTAIN_QUANTILE: f64 = 0.6;

/// The world map's location names (the spelling reference for OCR) and their categories.
const LOCATIONS: &[(&str, IconType)] = &[
    ("Denver", IconType::City), ("Boulder", IconType::City), ("Jericho", IconType::City),
    ("Burham Springs", IconType::City), ("Bloomfield", IconType::City),
    ("Blackfoot", IconType::Settlement), ("Twin Mothers", IconType::Settlement),
    ("Reservation", IconType::Settlement), ("Fort Abandon", IconType::Settlement),
    ("Hoover Dam", IconType::Landmark), ("Grand Canyon", IconType::Landmark), ("Mesa Verde", IconType::Landmark),
    ("Tibbets Prison", IconType::Landmark), ("Maxon's Bunker", IconType::Landmark),
    ("Nursery", IconType::Landmark), ("Ouroborus", IconType::Landmark),
];

/// Real coordinates of the drawn state borders: the two vertical lines (west to east) and the
/// two horizontal lines (north to south).
const BORDER_LONS: [f32; 2] = [-114.05, -109.045]; // NV/UT-AZ, UT/CO-AZ/NM
const BORDER_LATS: [f32; 2] = [41.0, 37.0];        // UT/CO-WY, UT/CO-AZ/NM

pub struct VanBurenExtractor;

impl Extractor for VanBurenExtractor {
    fn get_id(&self) -> &str { "van_buren" }
    fn get_code_path(&self) -> &str { "src/extractors/van_buren.rs" }
    fn get_cluster_id(&self) -> String { "van_buren".to_string() }
    fn get_source_paths(&self) -> Vec<String> {
        demo_path().map(|p| vec![p.to_string_lossy().into_owned()]).unwrap_or_default()
    }

    fn extract(&self) -> RawMapData {
        match extract_world_map() {
            Ok(data) => data,
            Err(e) => {
                warn!("Van Buren extraction failed: {}", e);
                RawMapData { warnings: vec![format!("Van Buren extraction failed: {}", e)], coordinate_scale: BLOCK_PX as f32, priority: crate::schema::tier::SPIN_OFF, ..Default::default() }
            }
        }
    }
}

fn err(msg: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, msg.into()) }

/// The demo RAR: `paths.json` key `van_buren`, else the cache (downloaded on first use).
fn demo_path() -> io::Result<PathBuf> {
    if let Some(p) = crate::utils::load_config_paths().unwrap_or_default().get("van_buren") {
        return Ok(PathBuf::from(p));
    }
    let cache = Path::new(CACHE_RAR);
    if !cache.exists() {
        info!("[Downloader] fetching {}...", DEMO_URL);
        // The host rejects some default user agents.
        let resp = ureq::get(DEMO_URL).set("User-Agent", "curl/8.0").call()
            .map_err(|e| err(format!("download {}: {}", DEMO_URL, e)))?;
        std::fs::create_dir_all(cache.parent().unwrap())?;
        let tmp = cache.with_extension("part");
        io::copy(&mut resp.into_reader(), &mut std::fs::File::create(&tmp)?)?;
        std::fs::rename(&tmp, cache)?;
    }
    Ok(cache.to_path_buf())
}

/// Reads the named files out of the RAR (names compared case-insensitively, `/` separators).
fn read_from_rar(rar: &Path, wanted: &[&str]) -> io::Result<Vec<Vec<u8>>> {
    let mut out = vec![None; wanted.len()];
    let mut archive = unrar::Archive::new(rar).open_for_processing().map_err(|e| err(e.to_string()))?;
    while let Some(header) = archive.read_header().map_err(|e| err(e.to_string()))? {
        let name = header.entry().filename.to_string_lossy().replace('\\', "/").to_lowercase();
        archive = match wanted.iter().position(|w| w.to_lowercase() == name) {
            Some(i) => { let (bytes, next) = header.read().map_err(|e| err(e.to_string()))?; out[i] = Some(bytes); next }
            None => header.skip().map_err(|e| err(e.to_string()))?,
        };
    }
    out.into_iter().zip(wanted).map(|(b, w)| b.ok_or_else(|| err(format!("{} not in archive", w)))).collect()
}

/// Finds a resource by name via `resource.rht` and returns its bytes from the matching GRP.
/// RHT: 5 x i32 header (vMajor, vMinor, nEntries, offsetPacks, offsetNames), then nEntries x 5 x i32
/// (one, local index, type, name offset, pack-name offset). GRP: u32 version, u32, u32 count,
/// then count x (u32 offset, u32 size).
fn grp_resource<'a>(rht: &[u8], grp_name: &str, grp: &'a [u8], name: &str) -> io::Result<&'a [u8]> {
    let i32_at = |b: &[u8], o: usize| b.get(o..o + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap()));
    let cstr = |b: &'_ [u8], o: usize| -> String {
        let s = &b[o.min(b.len())..];
        String::from_utf8_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]).into_owned()
    };
    let n = i32_at(rht, 8).ok_or_else(|| err("bad RHT"))? as usize;
    let (packs, names) = (i32_at(rht, 12).unwrap() as usize, i32_at(rht, 16).unwrap() as usize);
    for e in 0..n {
        let base = 20 + e * 20;
        let (local, name_off, pack_off) = (i32_at(rht, base + 4).unwrap() as usize, i32_at(rht, base + 12).unwrap() as usize, i32_at(rht, base + 16).unwrap() as usize);
        if cstr(rht, names + name_off).eq_ignore_ascii_case(name) && cstr(rht, packs + pack_off).eq_ignore_ascii_case(grp_name) {
            let at = 12 + local * 8;
            let (off, size) = (i32_at(grp, at).ok_or_else(|| err("bad GRP"))? as usize, i32_at(grp, at + 4).unwrap() as usize);
            return grp.get(off..off + size).ok_or_else(|| err("GRP entry out of range"));
        }
    }
    Err(err(format!("{} not found in {}", name, grp_name)))
}

pub(crate) fn load_world_map() -> io::Result<RgbImage> {
    let files = read_from_rar(&demo_path()?, &["resource.rht", "data/Interface.grp"])?;
    let tga = grp_resource(&files[0], "Interface", &files[1], "F3_WorldMap")?;
    Ok(image::load_from_memory_with_format(tga, image::ImageFormat::Tga).map_err(|e| err(e.to_string()))?.to_rgb8())
}

fn is_ring_green(p: [u8; 3]) -> bool { p[1] > 150 && p[0] < 130 && p[2] < 130 }

/// Centres of the 46x46 location rings (connected green components of that size).
pub(crate) fn find_rings(img: &RgbImage) -> Vec<(u32, u32)> {
    let (w, h) = img.dimensions();
    let mut seen = vec![false; (w * h) as usize];
    let mut rings = Vec::new();
    for start in 0..(w * h) {
        if seen[start as usize] || !is_ring_green(img.as_raw()[start as usize * 3..][..3].try_into().unwrap()) { continue; }
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        let mut stack = vec![start];
        seen[start as usize] = true;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            for dy in -1i64..=1 { for dx in -1i64..=1 {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 { continue; }
                let j = (ny as u32 * w + nx as u32) as usize;
                if !seen[j] && is_ring_green(img.get_pixel(nx as u32, ny as u32).0) { seen[j] = true; stack.push(j as u32); }
            }}
        }
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        if (44..=48).contains(&bw) && (44..=48).contains(&bh) { rings.push(((x0 + x1 + 1) / 2, (y0 + y1 + 1) / 2)); }
    }
    rings.sort_by_key(|&(x, y)| (y, x));
    rings
}

/// OCR of the label drawn under a ring (text strongly greener than its background).
fn read_label(img: &RgbImage, cx: u32, cy: u32) -> io::Result<String> {
    const SCALE: u32 = 4;
    let (x0, y0) = (cx.saturating_sub(180), cy + 20);
    let (cw, ch) = ((cx + 180).min(img.width()) - x0, 24.min(img.height().saturating_sub(y0)));
    let pad = 20;
    let mut crop = image::GrayImage::from_pixel(cw * SCALE + 2 * pad, ch * SCALE + 2 * pad, image::Luma([255]));
    for y in 0..ch * SCALE { for x in 0..cw * SCALE {
        let p = img.get_pixel(x0 + x / SCALE, y0 + y / SCALE).0;
        if p[1] as i32 - (p[0].max(p[2]) as i32) > 40 { crop.put_pixel(x + pad, y + pad, image::Luma([0])); }
    }}
    let crop = image::imageops::blur(&crop, 1.2);
    let mut png = Vec::new();
    image::DynamicImage::ImageLuma8(crop).write_to(&mut io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| err(e.to_string()))?;
    let mut child = Command::new("tesseract").args(["stdin", "stdout", "--psm", "7", "-l", "eng"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
        .map_err(|e| err(format!("tesseract not available: {}", e)))?;
    child.stdin.take().unwrap().write_all(&png)?;
    let mut text = String::new();
    child.stdout.take().unwrap().read_to_string(&mut text)?;
    child.wait()?;
    Ok(text.trim().to_string())
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() { cur.push((prev[j] + (ca != cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1)); }
        prev = cur;
    }
    prev[b.len()]
}

/// Assigns each OCR text to the closest known location name (letters only, case-insensitive),
/// best matches first, each name used once. Texts too far from every name stay unassigned.
pub(crate) fn snap_names(texts: &[String]) -> Vec<Option<usize>> {
    let key = |s: &str| s.chars().filter(|c| c.is_ascii_alphabetic()).map(|c| c.to_ascii_lowercase()).collect::<Vec<_>>();
    let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
    for (t, text) in texts.iter().enumerate() {
        let k = key(text);
        for (n, (name, _)) in LOCATIONS.iter().enumerate() {
            let nk = key(name);
            pairs.push((levenshtein(&k, &nk) as f32 / nk.len() as f32, t, n));
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut out, mut used) = (vec![None; texts.len()], vec![false; LOCATIONS.len()]);
    for (d, t, n) in pairs {
        if d <= 0.6 && out[t].is_none() && !used[n] { out[t] = Some(n); used[n] = true; }
    }
    out
}

/// Long yellow border lines: columns/rows with many yellow pixels that are not on the 50 px grid.
/// Returns (vertical x positions, horizontal y positions), each sorted.
pub(crate) fn find_borders(img: &RgbImage) -> (Vec<u32>, Vec<u32>) {
    let (w, h) = img.dimensions();
    let yellow = |p: [u8; 3]| p[0] > 150 && p[1] > 120 && p[2] < 90 && (p[0] as i32 - p[1] as i32).abs() < 70;
    let (mut cols, mut rows) = (vec![0u32; w as usize], vec![0u32; h as usize]);
    for (x, y, p) in img.enumerate_pixels() { if yellow(p.0) { cols[x as usize] += 1; rows[y as usize] += 1; } }
    let lines = |counts: &[u32], len: u32| -> Vec<u32> {
        let mut found: Vec<u32> = Vec::new();
        for (i, &c) in counts.iter().enumerate() {
            let on_grid = matches!(i % 50, 0 | 1 | 49);
            if !on_grid && c > len / 4 {
                match found.last_mut() { Some(l) if i as u32 - *l <= 2 => {}, _ => found.push(i as u32) }
            }
        }
        found
    };
    (lines(&cols, h), lines(&rows, w))
}

fn extract_world_map() -> io::Result<RawMapData> {
    info!("Extracting Van Buren world map...");
    let img = load_world_map()?;
    let (w, h) = img.dimensions();
    let mut warnings = Vec::new();

    // Markers
    let mut locations = Vec::new();
    let rings = find_rings(&img);
    let texts = rings.iter().map(|&(x, y)| read_label(&img, x, y)).collect::<io::Result<Vec<_>>>()?;
    for ((&(x, y), text), name) in rings.iter().zip(&texts).zip(snap_names(&texts)) {
        match name {
            Some(n) => locations.push(RawLocation {
                name: LOCATIONS[n].0.to_string(), x: x as f32, y: y as f32, icon_type: LOCATIONS[n].1,
                real_lat: None, real_lon: None, terrain_override: None, anchor: None,
            }),
            None => warnings.push(format!("Van Buren: unreadable location label {:?} at ({}, {})", text, x, y)),
        }
    }

    // Anchors from the drawn borders
    let (xs, ys) = find_borders(&img);
    let mut control_points = Vec::new();
    if xs.len() == 2 && ys.len() == 2 {
        for (xi, yi, name) in [(1, 1, "Four Corners"), (0, 1, "NV-UT-AZ corner"), (1, 0, "CO-UT-WY corner")] {
            control_points.push(RawControlPoint { name: name.into(), x: xs[xi] as f32, y: ys[yi] as f32, lat: BORDER_LATS[yi], lon: BORDER_LONS[xi] });
        }
    } else {
        warnings.push(format!("Van Buren: expected 2 vertical and 2 horizontal borders, found {:?} / {:?}", xs, ys));
    }

    // Terrain
    let tiles = classify_terrain(&img);
    info!("Van Buren: {} locations, {} anchors, {} tiles", locations.len(), control_points.len(), tiles.len());
    Ok(RawMapData {
        locations, tiles,
        width: w / BLOCK_PX as u32, height: h / BLOCK_PX as u32,
        coordinate_scale: BLOCK_PX as f32,
        priority: crate::schema::tier::SPIN_OFF,
        control_points,
        warnings,
        ..Default::default()
    })
}

/// Interstates are drawn dark maroon (about 48-72, 32-40, 32-40): red well above green, green ~ blue.
fn is_road(p: [u8; 3]) -> bool {
    let [r, g, b] = p.map(|v| v as i32);
    r >= 40 && r * 100 > g * 135 && (g - b).abs() <= 12
}
fn is_teal(p: [u8; 3]) -> bool {
    let [r, g, b] = p.map(|v| v as i32);
    g > r + 20 && b > r + 10 && g < 200 && !is_ring_green(p)
}


/// Water from the teal lakes/rivers, Mountain where local relief is in the top
/// (1 - MOUNTAIN_QUANTILE) of the map, Sand elsewhere. Drawn overlays (rings, labels, borders,
/// roads, grid) are excluded from the relief measure.
fn classify_terrain(img: &RgbImage) -> Vec<RawTile> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let px = |i: usize| { let p = img.as_raw(); [p[i * 3] as i32, p[i * 3 + 1] as i32, p[i * 3 + 2] as i32] };
    let mut lum = vec![0.0; w * h];
    let mut keep = vec![0.0; w * h];
    let mut teal = vec![0.0; w * h];
    for i in 0..w * h {
        let [r, g, b] = px(i);
        let green = g > 150 && r < 130 && b < 130;
        let yellow = r > 150 && g > 120 && b < 90 && (r - g).abs() < 70;
        let road = is_road([r as u8, g as u8, b as u8]);
        if is_teal([r as u8, g as u8, b as u8]) { teal[i] = 1.0; }
        if !(green || yellow || road) { keep[i] = 1.0; lum[i] = (r + g + b) as f64 / 3.0; }
    }
    // Overlay pixels take the mean of nearby kept pixels (normalised convolution), then relief = 9x9 std.
    let (kl, k) = (crate::raster::box_mean(&lum, w, h, 3), crate::raster::box_mean(&keep, w, h, 3));
    let filled: Vec<f64> = (0..w * h).map(|i| if keep[i] > 0.0 { lum[i] } else if k[i] > 0.0 { kl[i] / k[i] } else { 0.0 }).collect();
    let sq: Vec<f64> = filled.iter().map(|v| v * v).collect();
    let (m, m2) = (crate::raster::box_mean(&filled, w, h, 4), crate::raster::box_mean(&sq, w, h, 4));
    let relief: Vec<f64> = m.iter().zip(&m2).map(|(a, b)| (b - a * a).max(0.0).sqrt()).collect();

    let (bw, bh) = (w / BLOCK_PX, h / BLOCK_PX);
    let mut block_relief = vec![0.0; bw * bh];
    let mut block_teal = vec![0.0; bw * bh];
    for by in 0..bh { for bx in 0..bw {
        let (mut r, mut t) = (0.0, 0.0);
        for y in 0..BLOCK_PX { for x in 0..BLOCK_PX {
            let i = (by * BLOCK_PX + y) * w + bx * BLOCK_PX + x;
            r += relief[i]; t += teal[i];
        }}
        let n = (BLOCK_PX * BLOCK_PX) as f64;
        block_relief[by * bw + bx] = r / n;
        block_teal[by * bw + bx] = t / n;
    }}
    let mut sorted = block_relief.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let threshold = sorted[(sorted.len() as f64 * MOUNTAIN_QUANTILE) as usize];

    (0..bw * bh).map(|i| RawTile {
        x: (i % bw) as i32, y: (i / bw) as i32,
        tile_type: if block_teal[i] > 0.4 { "Water" } else if block_relief[i] > threshold { "Mountain" } else { "Sand" }.to_string(),
        real_lat: None, real_lon: None, weight: 1.0, anchor: None, lod: false,
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_world_map_locations_and_borders() {
        let img = match load_world_map() {
            Ok(i) => i,
            Err(e) => { eprintln!("skipping: Van Buren demo unavailable ({})", e); return; }
        };
        assert_eq!(img.dimensions(), (2151, 1951));
        let rings = find_rings(&img);
        assert_eq!(rings.len(), 16, "rings: {:?}", rings);
        let texts: Vec<String> = rings.iter().map(|&(x, y)| read_label(&img, x, y).unwrap()).collect();
        let names = snap_names(&texts);
        for ((r, t), n) in rings.iter().zip(&texts).zip(&names) {
            println!("{:?} {:<22} -> {:?}", r, t, n.map(|n| LOCATIONS[n].0));
        }
        assert!(names.iter().all(|n| n.is_some()), "every label must resolve to a name");
        // Spot-check positions against the known rings.
        let at = |name: &str| rings[names.iter().position(|n| n.map(|n| LOCATIONS[n].0) == Some(name)).unwrap()];
        assert_eq!(at("Denver"), (2025, 525));
        assert_eq!(at("Hoover Dam"), (125, 1425));

        let (xs, ys) = find_borders(&img);
        assert_eq!(xs.len(), 2, "vertical borders {:?}", xs);
        assert_eq!(ys.len(), 2, "horizontal borders {:?}", ys);
        assert!(xs[0].abs_diff(260) <= 2 && xs[1].abs_diff(1230) <= 2, "{:?}", xs);
        assert!(ys[0].abs_diff(181) <= 2 && ys[1].abs_diff(1162) <= 2, "{:?}", ys);

    }
}

