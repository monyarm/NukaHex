//! Fallout Tactics: the campaign world map (`campaigns/bos.cam` in `core/Mis-Main_0.bos`).
//!
//! `.bos` archives are plain ZIPs. `bos.cam` is a tagged binary:
//! - `<campaign>\0` `19\0\0`, then u32 columns, u32 rows, u32 tile size in px (65 x 34 x 75)
//! - one `<campaign_tile>` per tile (encounter odds, not terrain) followed by its `<zar>` art,
//!   stored column-major
//! - `<esh>` property lists for the locations: Name (a locale key), Loc x/y (px), Is Bunker, ...
//! Location names come from `locale/game/generic_mission.txt` in `core/locale_0.bos`.
//!
//! Mods (tier 6, fetched by `crate::mods`) ship their own campaign file (`*.cam`) and locale loose.

use crate::extractors::Extractor;
use crate::schema::{IconType, RawLocation, RawMapData, RawTile};
use log::{info, warn};
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::PathBuf;

const STEAM_APP_ID: u32 = 38420;

/// Fallout Tactics' own campaign, or a mod's (its id in `input/config/mods.json`).
pub enum TacticsExtractor {
    Main,
    /// A mod's campaign (tier 6): terrain only, its markers anchor the warp.
    Mod(&'static str),
}

impl Extractor for TacticsExtractor {
    fn get_id(&self) -> &str { match self { Self::Main => "fallout_tactics", Self::Mod(id) => id } }
    fn get_code_path(&self) -> &str { "src/extractors/tactics.rs" }
    fn get_cluster_id(&self) -> String {
        match self { Self::Main => "fallout_tactics".to_string(), Self::Mod(id) => format!("mod_fallout_tactics_{}", id) }
    }
    fn get_source_paths(&self) -> Vec<String> {
        match self {
            Self::Main => install_dir().map(|d| vec![d.join("core/Mis-Main_0.bos").to_string_lossy().into_owned()]).unwrap_or_default(),
            Self::Mod(_) => vec!["input/config/mods.json".to_string()],
        }
    }

    fn extract(&self) -> RawMapData {
        let tier = match self { Self::Main => crate::schema::tier::SPIN_OFF, Self::Mod(_) => crate::schema::tier::MODS };
        let result = match self {
            Self::Main => load_campaign().and_then(|c| {
                let locale = read_zip_entry(&install_dir()?.join("core/locale_0.bos"), "locale/game/generic_mission.txt")?;
                Ok((c, locale))
            }),
            Self::Mod(id) => load_mod_campaign(id),
        };
        match result.and_then(|(campaign, locale)| map_of(campaign, &String::from_utf8_lossy(&locale), self.get_id())) {
            Ok(data) => RawMapData { priority: tier, anchor_only_markers: matches!(self, Self::Mod(_)), ..data },
            Err(e) => {
                warn!("{} extraction failed: {}", self.get_id(), e);
                RawMapData { warnings: vec![format!("{} extraction failed: {}", self.get_id(), e)], coordinate_scale: 75.0, priority: tier, ..Default::default() }
            }
        }
    }
}

/// A fetched mod's campaign file and its locale's location names (the first `*.cam` and
/// `locale/game/generic_mission.txt` in its files, any case).
fn load_mod_campaign(id: &str) -> io::Result<(Campaign, Vec<u8>)> {
    let root = crate::mods::fetch(id).map_err(io::Error::other)?;
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for p in std::fs::read_dir(&dir)?.flatten().map(|e| e.path()) {
            if p.is_dir() { stack.push(p) } else { files.push(p) }
        }
    }
    files.sort();
    let lower = |p: &PathBuf| p.to_string_lossy().replace('\\', "/").to_lowercase();
    let cam = files.iter().find(|p| lower(p).ends_with(".cam")).ok_or_else(|| err("no campaign file (*.cam)"))?;
    let locale = files.iter().find(|p| lower(p).ends_with("locale/game/generic_mission.txt")).ok_or_else(|| err("no locale/game/generic_mission.txt"))?;
    Ok((parse_campaign(&std::fs::read(cam)?)?, std::fs::read(locale)?))
}

fn err(msg: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, msg.into()) }

fn install_dir() -> io::Result<PathBuf> {
    if let Some(p) = crate::utils::load_config_paths().unwrap_or_default().get("fallout_tactics") {
        return Ok(PathBuf::from(p));
    }
    crate::dat_extractor::find_steam_app(STEAM_APP_ID, "Fallout Tactics")
}

fn read_zip_entry(archive: &std::path::Path, name: &str) -> io::Result<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?).map_err(|e| err(e.to_string()))?;
    let mut entry = zip.by_name(name).map_err(|e| err(format!("{} in {}: {}", name, archive.display(), e)))?;
    let mut out = Vec::new();
    entry.read_to_end(&mut out)?;
    Ok(out)
}

/// The parsed campaign: grid size, tile size, locations (locale key, x, y, is_bunker), stitched art.
pub(crate) struct Campaign {
    pub cols: usize,
    pub rows: usize,
    pub tile_px: usize,
    pub locations: Vec<(String, f32, f32, bool)>,
    pub art: Vec<[u8; 3]>,
}

pub(crate) fn load_campaign() -> io::Result<Campaign> {
    let cam = read_zip_entry(&install_dir()?.join("core/Mis-Main_0.bos"), "campaigns/bos.cam")?;
    parse_campaign(&cam)
}

fn find_all(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    hay.windows(needle.len()).enumerate().filter(|(_, w)| *w == needle).map(|(i, _)| i).collect()
}

pub(crate) fn parse_campaign(cam: &[u8]) -> io::Result<Campaign> {
    let u32_at = |o: usize| cam.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()) as usize).ok_or_else(|| err("truncated campaign"));
    if !cam.starts_with(b"<campaign>\0") { return Err(err("not a campaign file")); }
    // "<campaign>\0" (11) + "19\0\0" (4)
    let (cols, rows, tile_px) = (u32_at(15)?, u32_at(19)?, u32_at(23)?);
    if cols * rows * tile_px * tile_px > 64 << 20 { return Err(err(format!("implausible campaign size {}x{}x{}", cols, rows, tile_px))); }

    // Art: one ZAR per tile, column-major.
    let (w, h) = (cols * tile_px, rows * tile_px);
    let mut art = vec![[0u8; 3]; w * h];
    let zars = find_all(cam, b"<zar>");
    if zars.len() < cols * rows { return Err(err(format!("expected {} tile images, found {}", cols * rows, zars.len()))); }
    for (i, &at) in zars.iter().take(cols * rows).enumerate() {
        let (tw, th, px) = crate::zar::decode(&cam[at..]).ok_or_else(|| err(format!("bad tile image {}", i)))?;
        if (tw, th) != (tile_px, tile_px) { return Err(err(format!("tile {} is {}x{}", i, tw, th))); }
        let (ox, oy) = ((i / rows) * tile_px, (i % rows) * tile_px);
        for y in 0..th { art[(oy + y) * w + ox..(oy + y) * w + ox + tw].copy_from_slice(&px[y * tw..(y + 1) * tw]); }
    }

    // Locations: `<esh>\0` "1\0" u32 n, then n x (u32 len, key, u32 type, u32 size, payload).
    let mut locations = Vec::new();
    for at in find_all(cam, b"<esh>\0") {
        let mut p = at + 8;
        let Ok(n) = u32_at(p) else { continue };
        p += 4;
        let mut props: HashMap<String, Vec<u8>> = HashMap::new();
        for _ in 0..n {
            let (Ok(kl), Some(_)) = (u32_at(p), cam.get(p + 4)) else { break };
            let key = String::from_utf8_lossy(&cam[p + 4..(p + 4 + kl).min(cam.len())]).into_owned();
            p += 4 + kl;
            let (Ok(_ty), Ok(size)) = (u32_at(p), u32_at(p + 4)) else { break };
            p += 8;
            props.insert(key, cam.get(p..p + size).unwrap_or_default().to_vec());
            p += size;
        }
        let int = |k: &str| props.get(k).filter(|v| v.len() >= 4).map(|v| i32::from_le_bytes(v[..4].try_into().unwrap()));
        let name = props.get("Name").and_then(|v| v.get(4..)).map(|s| String::from_utf8_lossy(s).into_owned());
        if let (Some(name), Some(x), Some(y)) = (name, int("Loc x"), int("Loc y")) {
            let bunker = props.get("Is Bunker").is_some_and(|v| v.first() == Some(&1));
            locations.push((name, x as f32, y as f32, bunker));
        }
    }
    Ok(Campaign { cols, rows, tile_px, locations, art })
}

/// `key = {Text}` lines of a Tactics locale file.
fn parse_locale(text: &str) -> HashMap<String, String> {
    text.lines().filter_map(|l| {
        let (k, v) = l.split_once('=')?;
        let v = v.trim().strip_prefix('{')?.split('}').next()?;
        Some((k.trim().to_string(), v.trim().to_string()))
    }).collect()
}

/// Category of a campaign location (all are major story sites).
fn marker_type(name: &str, bunker: bool) -> IconType {
    match name {
        _ if bunker => IconType::Landmark, // Brotherhood bunkers
        "Vault 0" => IconType::Vault,
        "Cheyenne Mountain" => IconType::Landmark,
        "Brahmin Wood" => IconType::Settlement,
        _ => IconType::City,
    }
}

/// The map of a campaign whose locale (`generic_mission.txt`) is `locale`; `id` names it in messages.
fn map_of(campaign: Campaign, locale: &str, id: &str) -> io::Result<RawMapData> {
    info!("Extracting {} campaign map...", id);
    let locale = parse_locale(locale);

    let mut warnings = Vec::new();
    let locations = campaign.locations.iter().filter_map(|(key, x, y, bunker)| {
        let Some(name) = locale.get(key) else {
            warnings.push(format!("{}: no name for location key '{}'", id, key));
            return None;
        };
        Some(RawLocation {
            name: name.clone(), x: *x, y: *y, icon_type: marker_type(name, *bunker),
            real_lat: None, real_lon: None, terrain_override: None, anchor: None,
        })
    }).collect::<Vec<_>>();

    let (w, h) = (campaign.cols * campaign.tile_px, campaign.rows * campaign.tile_px);
    let (tiles, block) = classify_terrain(&campaign.art, w, h);
    info!("{}: {} locations, {} tiles", id, locations.len(), tiles.len());
    Ok(RawMapData {
        locations, tiles,
        width: (w / block) as u32, height: (h / block) as u32,
        coordinate_scale: block as f32,
        warnings,
        ..Default::default()
    })
}

/// Terrain blocks (BLOCK px) from the campaign art. The painting has real terrain colour:
/// - Snow: near-white (the Rockies)
/// - Water: smooth pale cream (Lake Michigan, the Mississippi lakes), or blue (a mod's ocean)
/// - Mountain: large-scale relief texture well above the map's typical (`MOUNTAIN_RELIEF`), so a flat
///   map has little
/// - Grass: green land (`GREEN_HUE`; the olive plains of Kansas/Missouri), by (g-b)/(r-b) hue
/// - Sand: everything else (brown land)
/// Each block takes the majority of its pixels' classes.
/// Relief, as a multiple of the map's median, from which ground is Mountain (the main campaign's ranges:
/// about its strongest 20%).
const MOUNTAIN_RELIEF: f64 = 1.3;
/// (g-b)/(r-b) hue from which land is Grass: the main campaign's olive plains (its greenest 30%), a colour
/// that means the same on a mod's map.
const GREEN_HUE: f64 = 0.61;

fn classify_terrain(art: &[[u8; 3]], w: usize, h: usize) -> (Vec<RawTile>, usize) {
    use crate::raster::box_mean;
    const BLOCK: usize = 15;
    let lum: Vec<f64> = art.iter().map(|p| (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0).collect();
    let lum2: Vec<f64> = lum.iter().map(|l| l * l).collect();
    let (m, m2) = (box_mean(&lum, w, h, 4), box_mean(&lum2, w, h, 4));
    let std: Vec<f64> = m.iter().zip(&m2).map(|(a, b)| (b - a * a).max(0.0).sqrt()).collect();
    let relief = box_mean(&std, w, h, 30);
    let smooth = box_mean(&std, w, h, 7);
    let hue: Vec<f64> = art.iter().map(|p| (p[1] as f64 - p[2] as f64) / (p[0] as f64 - p[2] as f64).max(10.0)).collect();
    let green = box_mean(&hue, w, h, 30);
    let snow = box_mean(&lum.iter().map(|&l| (l > 205.0) as u8 as f64).collect::<Vec<_>>(), w, h, 4);
    let blue = |p: [u8; 3]| p[2] as f64 > p[0] as f64 + 15.0 && p[2] > p[1];
    let water_px: Vec<f64> = (0..w * h).map(|i| ((lum[i] > 140.0 && lum[i] <= 205.0 && smooth[i] < 9.0) || blue(art[i])) as u8 as f64).collect();
    let water = box_mean(&water_px, w, h, 7);

    let quantile = |v: &[f64], q: f64| { let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect(); s.sort_by(|a, b| a.total_cmp(b)); s[((s.len() - 1) as f64 * q) as usize] };
    let (relief_t, green_t) = (MOUNTAIN_RELIEF * quantile(&relief, 0.5), GREEN_HUE);
    let class = |i: usize| -> &'static str {
        if snow[i] > 0.5 { "Snow" } else if water[i] > 0.5 { "Water" } else if relief[i] > relief_t { "Mountain" } else if green[i] > green_t { "Grass" } else { "Sand" }
    };

    let (bw, bh) = (w / BLOCK, h / BLOCK);
    let tiles = (0..bw * bh).map(|b| {
        let (bx, by) = (b % bw, b / bw);
        let mut votes: HashMap<&str, usize> = HashMap::new();
        for y in (0..BLOCK).step_by(3) { for x in (0..BLOCK).step_by(3) {
            *votes.entry(class((by * BLOCK + y) * w + bx * BLOCK + x)).or_default() += 1;
        }}
        let t = votes.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0))).unwrap().0;
        RawTile { x: bx as i32, y: by as i32, tile_type: t.into(), real_lat: None, real_lon: None, weight: 1.0, anchor: None, lod: false }
    }).collect();
    (tiles, BLOCK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_campaign_map() {
        let c = match load_campaign() {
            Ok(c) => c,
            Err(e) => { eprintln!("skipping: Fallout Tactics unavailable ({})", e); return; }
        };
        assert_eq!((c.cols, c.rows, c.tile_px), (65, 34, 75));
        assert_eq!(c.locations.len(), 27, "{:?}", c.locations);
        assert_eq!(c.locations.iter().filter(|l| l.3).count(), 5, "bunkers");
        let st_louis = c.locations.iter().find(|l| l.0 == "mission_name_08").expect("St. Louis");
        assert_eq!((st_louis.1, st_louis.2), (3890.0, 1756.0));
    }
}
