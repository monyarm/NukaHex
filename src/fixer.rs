use crate::schema::{RawMapData, FixedMapData, FixedTile, FixedLocation, priority_weight};
use crate::rubbersheeting::{RubberSheet, ControlPoint};
use crate::geocoding;
use std::collections::{HashMap, HashSet};
use log::{info, warn};

// User-provided real-world coordinates
// Each entry can be either:
// - Numeric coordinates: ("Name", "37.376,-118.482")
// - Place name/address: ("Name", "Griffith Observatory, Los Angeles")
const REAL_COORDS: &[(&str, &str)] = &[
    // (name, coordinates_or_address)
    ("Shady Sands", "37.3767525,-118.4820828"),
    ("Mariposa Military Base", "36.624861300000006,-121.78344730000002"), 
    ("Lost Hill Bunker", "35.6081873,-119.3829345"),  
    ("Junktown", "35.65363980000001,-117.4329572"),
    ("The Hub", "34.1391501,-117.2897652"),  
    ("Necropolis", "34.913266,-117.015381"),  
    ("The Boneyard", "34.2070709,-118.3117676"),
    ("The Cathedral", "33.8088202,-118.32000730000001"),
    ("The Glow", "33.1467502,-116.85058590000001"),
    ("Vault 13", "37.3679428,-118.9736945"),
    ("Vault 15", "37.3964654,-118.2568593"),
    // Fallout 2
    ("Navarro", "38.8623645,-123.485783"),
    ("San Francisco", "37.7924224,-122.42614750000001"), 
    ("Redding", "40.5847565,-122.3876953"),
    ("Klamath", "41.5281142,-124.03907770000002"),
    ("Vault City", "41.1793239,-119.29021499999999"),
    ("Gecko", "41.7063278,-118.93786470000002"),
    ("Modoc", "41.6727239,-120.899064"),
    ("New Reno", "39.5294665,-119.89379880000001"),
    ("Broken Hills", "39.5380233,-118.9489428"),
    // Van Buren (real places on its world map; the rest are fictional and placed by the warp)
    ("Denver", "39.7392,-104.9903"),
    ("Boulder", "40.0150,-105.2705"),
    ("Hoover Dam", "36.0160,-114.7377"),
    ("Grand Canyon", "36.0544,-112.1401"), // Grand Canyon Village, South Rim
    ("Mesa Verde", "37.1838,-108.4887"),   // Mesa Verde NP, Chapin Mesa
    // Fallout Tactics (keys are the game's spellings; values the real towns). Not anchored:
    // Newton and Canyon City, whose in-game positions are 170-210 mi from their namesakes
    // (Canyon City sits in western Kansas); the warp places them.
    ("Freeport", "42.2967,-89.6212"),
    ("Rock Falls", "41.7795,-89.6887"),
    ("Macomb", "40.4592,-90.6718"),
    ("Preoria", "40.6936,-89.5890"),          // Peoria, IL
    ("Quincy", "39.9356,-91.4099"),
    ("Springfield", "39.7817,-89.6501"),
    ("St.Louis", "38.6270,-90.1994"),
    ("Jefferson", "38.5767,-92.1735"),        // Jefferson City, MO
    ("Kansas City", "39.0997,-94.5786"),
    ("Osceolla", "38.0467,-93.7044"),         // Osceola, MO
    ("Junction City", "39.0286,-96.8314"),
    ("Great Bend", "38.3645,-98.7648"),
    ("Coldwater", "37.2689,-99.3268"),
    ("Scott City", "38.4825,-100.9071"),
    ("Beuna Vista", "38.8422,-106.1311"),     // Buena Vista, CO
    ("Cheyenne Mountain", "38.7445,-104.8463"),
    // Fallout 4
    ("Diamond City", "42.346676,-71.097218"), // Fenway Park
    ("Goodneighbor", "42.358433,-71.057530"), // Old State House
    ("Boston Public Library", "42.349395,-71.076615"),
    ("Bunker Hill", "42.376365,-71.060773"),
    ("USS Constitution", "42.372439,-71.056586"),
    ("Old North Church", "42.366367,-71.054378"),
];

const MILES_PER_DEGREE_LAT: f32 = 69.0;

/// Hex rows (stagger on y) are sqrt(3)/2 of a column step apart, so a hex's neighbours are all
/// `miles_per_hex` away.
const HEX_ROW_FRACTION: f32 = 0.866_025_4;

/// Lambert conformal conic projection for the continental US (standard parallels 33N/45N, origin
/// 39N 96W): scale stays within ~1-2% of true across the mapped region and local shapes are kept.
mod lcc {
    const R_MILES: f64 = 3958.8;
    const PHI1: f64 = 33.0;
    const PHI2: f64 = 45.0;
    const PHI0: f64 = 39.0;
    const LAMBDA0: f64 = -96.0;
    fn t(phi_deg: f64) -> f64 { (std::f64::consts::FRAC_PI_4 + phi_deg.to_radians() / 2.0).tan() }
    fn consts() -> (f64, f64, f64) {
        let n = (PHI1.to_radians().cos() / PHI2.to_radians().cos()).ln() / (t(PHI2) / t(PHI1)).ln();
        let f = PHI1.to_radians().cos() * t(PHI1).powf(n) / n;
        (n, f, R_MILES * f / t(PHI0).powf(n))
    }
    /// (lat, lon) degrees -> (x east, y north) miles.
    pub fn forward(lat: f64, lon: f64) -> (f64, f64) {
        let (n, f, rho0) = consts();
        let rho = R_MILES * f / t(lat).powf(n);
        let theta = n * (lon - LAMBDA0).to_radians();
        (rho * theta.sin(), rho0 - rho * theta.cos())
    }
    /// (x, y) miles -> (lat, lon) degrees.
    pub fn inverse(x: f64, y: f64) -> (f64, f64) {
        let (n, f, rho0) = consts();
        let rho = n.signum() * (x * x + (rho0 - y).powi(2)).sqrt();
        let theta = (x / rho).atan2((rho0 - y) / rho);
        let lat = 2.0 * (R_MILES * f / rho).powf(1.0 / n).atan() - std::f64::consts::FRAC_PI_2;
        (lat.to_degrees(), LAMBDA0 + (theta / n).to_degrees())
    }
}

/// The output-grid convention, used by every stage: continuous (col, row) of a lat/lon on the
/// world grid at `miles_per_hex` (centre-to-centre distance between neighbouring hexes), via the
/// `lcc` projection. Rows grow south; integer indices are cell centres. (Each game's own
/// coordinate scale is separate: see `RawMapData::coordinate_scale`.)
pub fn to_grid(lat: f32, lon: f32, miles_per_hex: f32) -> (f32, f32) {
    let (x, y) = lcc::forward(lat as f64, lon as f64);
    ((x / miles_per_hex as f64) as f32, (-y / (miles_per_hex * HEX_ROW_FRACTION) as f64) as f32)
}

/// Inverse of `to_grid`: (lat, lon) at grid position (col, row).
pub fn from_grid(col: f32, row: f32, miles_per_hex: f32) -> (f32, f32) {
    let (lat, lon) = lcc::inverse(col as f64 * miles_per_hex as f64, -(row as f64) * (miles_per_hex * HEX_ROW_FRACTION) as f64);
    (lat as f32, lon as f32)
}

/// Region of a tile for border drawing: only real land tiles get one, so borders never run
/// through water or past the mapped area.
fn tile_region(tile_type: &str, lat: f32, lon: f32) -> Option<u16> {
    if tile_type == "unknown" || tile_type == "Water" { return None; }
    crate::borders::region_at(lat, lon)
}

/// Key of the 1-mile bin holding (lat, lon), for neighbour lookups of manual points.
fn bin_of(lat: f32, lon: f32) -> (i32, i32) {
    let (c, r) = to_grid(lat, lon, 1.0);
    (r.round() as i32, c.round() as i32)
}


use rayon::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub min_lat: f32,
    pub max_lat: f32,
    pub min_lon: f32,
    pub max_lon: f32,
}

pub fn fix(game_clusters: Vec<RawMapData>, manual_maps: Vec<RawMapData>, miles_per_hex: Option<f32>, bounds: Option<BoundingBox>) -> FixedMapData {
    info!("Running Global Fixer on {} game clusters and {} manual maps...", game_clusters.len(), manual_maps.len());

    // If miles_per_hex is 6.0 (global phase), use Translation-Only alignment
    let resolution = miles_per_hex.unwrap_or(6.0);
    if resolution >= 5.9 && resolution <= 6.1 {
        info!("Global Phase: Using Translation-Only alignment...");
        return fix_translation_only(game_clusters, manual_maps, resolution, bounds);
    }

    // fallback for other resolutions (e.g. city maps)
    let mut all_raw = game_clusters;
    all_raw.extend(manual_maps);
    
    let has_locations = all_raw.iter().any(|m| !m.locations.is_empty());
    let has_tiles = all_raw.iter().any(|m| !m.tiles.is_empty());
    
    let needs_warping = all_raw.iter().any(|m| m.coordinate_scale > 1.01 || m.coordinate_system == crate::schema::CoordinateSystem::Relative);
    
    if has_locations && has_tiles && needs_warping {
        return fix_with_terrain_inverse_warping(all_raw, resolution, bounds);
    }
    
    fix_legacy_pass_through(all_raw)
}

/// Placement for a Relative manual map: maps its grid (x, y) to (lat, lon) using its markers
/// that are already placed elsewhere (by name, lowercase key in `placed`) as anchors.
/// 1 anchor: translation at the map's own `miles_per_unit`. 2 anchors: similarity (uniform
/// scale + rotation) in a local miles frame. 3+: the same fold-free TPS used for games.
/// None (with a warning) when no anchor is placed yet.
fn relative_placement<'a>(map: &'a RawMapData, placed: &HashMap<String, (f32, f32)>, warnings: &mut Vec<String>)
    -> Option<Box<dyn Fn(f32, f32) -> (f32, f32) + Send + Sync + 'a>>
{
    let mut cps: Vec<ControlPoint> = map.locations.iter()
        .filter_map(|l| placed.get(&l.name.to_lowercase()).map(|&(lat, lon)| ControlPoint {
            name: l.name.clone(), game_x: l.x, game_y: l.y, real_lat: lat, real_lon: lon,
        }))
        .collect();
    cps.sort_by(|a, b| a.name.cmp(&b.name));
    cps.dedup_by(|a, b| a.game_x == b.game_x && a.game_y == b.game_y);

    let names: Vec<&str> = map.locations.iter().map(|l| l.name.as_str()).collect();
    if cps.is_empty() {
        let msg = format!("Relative map ({:?}...) has no placed anchors; skipped", &names[..names.len().min(3)]);
        warn!("{}", msg);
        warnings.push(msg);
        return None;
    }
    if cps.len() >= 3 {
        let area = (0.0, 0.0, map.width as f32, map.height as f32, 0.5);
        let (sheet, report) = RubberSheet::new_fold_free(cps, area);
        for msg in report {
            let msg = format!("Relative map {:?}: {}", &names[..names.len().min(3)], msg);
            warn!("{}", msg);
            warnings.push(msg);
        }
        return Some(Box::new(move |x, y| sheet.transform(x, y)));
    }

    // Local planar frame in miles (x east, y south), matching grid orientation.
    let lat0 = cps.iter().map(|c| c.real_lat).sum::<f32>() / cps.len() as f32;
    let kx = MILES_PER_DEGREE_LAT * lat0.to_radians().cos();
    let to_plane = move |lat: f32, lon: f32| (lon * kx, -lat * MILES_PER_DEGREE_LAT);
    let from_plane = move |e: f32, s: f32| (-s / MILES_PER_DEGREE_LAT, e / kx);
    let c0 = cps[0].clone();
    let p0 = to_plane(c0.real_lat, c0.real_lon);

    if cps.len() == 1 {
        let scale = map.miles_per_unit.unwrap_or_else(|| {
            warnings.push(format!("Relative map {:?} has one anchor and no miles_per_unit; assuming 6 mi/unit", &names[..names.len().min(3)]));
            6.0
        });
        return Some(Box::new(move |x, y| from_plane(p0.0 + (x - c0.game_x) * scale, p0.1 + (y - c0.game_y) * scale)));
    }

    // Two anchors: complex ratio a = (p1 - p0) / (g1 - g0); p = p0 + a * (g - g0).
    let c1 = &cps[1];
    let p1 = to_plane(c1.real_lat, c1.real_lon);
    let (gx, gy) = (c1.game_x - c0.game_x, c1.game_y - c0.game_y);
    let (px, py) = (p1.0 - p0.0, p1.1 - p0.1);
    let d = gx * gx + gy * gy;
    if d < 1e-6 { return Some(Box::new(move |_, _| (c0.real_lat, c0.real_lon))); }
    let (ar, ai) = ((px * gx + py * gy) / d, (py * gx - px * gy) / d);
    Some(Box::new(move |x, y| {
        let (dx, dy) = (x - c0.game_x, y - c0.game_y);
        from_plane(p0.0 + ar * dx - ai * dy, p0.1 + ai * dx + ar * dy)
    }))
}

/// New Global Aligner: Finds a single (dLat, dLon) offset per cluster based on an anchor.
pub fn fix_translation_only(game_clusters: Vec<RawMapData>, manual_maps: Vec<RawMapData>, resolution: f32, _bounds: Option<BoundingBox>) -> FixedMapData {
    info!("Aligning {} game clusters and {} manual maps via translation...", game_clusters.len(), manual_maps.len());
    
    let mut all_fixed_locations = Vec::new();
    let mut all_fixed_tiles = Vec::new();
    let mut warnings = Vec::new();

    // We'll project everything to lat/lon first
    for cluster in &game_clusters {
        warnings.extend(cluster.warnings.clone());
        // 1. Pick Anchor
        // User: "pick the first marker with proper lat/lon, or the first marker if there isn't one"
        let anchor_loc = cluster.locations.iter()
            .find(|l| l.real_lat.is_some() && l.real_lon.is_some())
            .or_else(|| cluster.locations.first());

        if let Some(anchor) = anchor_loc {
            // 2. Identify Target (Ideal) Position for Anchor
            let mut target_found = false;
            let (target_lat, target_lon) = if let (Some(la), Some(lo)) = (anchor.real_lat, anchor.real_lon) {
                let mut ideal_lat = la;
                let mut ideal_lon = lo;
                
                for (name, coords_str) in REAL_COORDS {
                    if name.eq_ignore_ascii_case(&anchor.name) {
                        if let Ok((la, lo)) = geocoding::parse_coords(coords_str) {
                            ideal_lat = la;
                            ideal_lon = lo;
                            target_found = true;
                            break;
                        }
                    }
                }
                (ideal_lat, ideal_lon)
            } else {
                let mut fallback_lat = 0.0;
                let mut fallback_lon = 0.0;
                for (name, coords_str) in REAL_COORDS {
                    if name.eq_ignore_ascii_case(&anchor.name) {
                        if let Ok((la, lo)) = geocoding::parse_coords(coords_str) {
                            fallback_lat = la;
                            fallback_lon = lo;
                            target_found = true;
                            break;
                        }
                    }
                }
                (fallback_lat, fallback_lon)
            };

            // 3. Calculate Translation Offset
            let current_lat = anchor.real_lat.unwrap_or(0.0);
            let current_lon = anchor.real_lon.unwrap_or(0.0);
            
            let (d_lat, d_lon) = if target_found {
                (target_lat - current_lat, target_lon - current_lon)
            } else {
                (0.0, 0.0) // Don't shift if not in our hardcoded California list
            };

            info!("Cluster aligned via anchor '{}' to offset (dLat: {:.4}, dLon: {:.4})", anchor.name, d_lat, d_lon);

            // Apply offset to all markers
            for loc in &cluster.locations {
                let (Some(lat), Some(lon)) = (loc.real_lat, loc.real_lon) else {
                    warnings.push(format!("Marker '{}' has no lat/lon after cluster fixing; not placed", loc.name));
                    continue;
                };
                all_fixed_locations.push((loc.name.clone(), lat + d_lat, lon + d_lon, loc.icon_type));
            }

            // Apply offset to all tiles
            for tile in &cluster.tiles {
                if let (Some(t_lat), Some(t_lon)) = (tile.real_lat, tile.real_lon) {
                    all_fixed_tiles.push((tile.tile_type.clone(), t_lat + d_lat, t_lon + d_lon, priority_weight(cluster.priority)));
                }
            }
        }
    }

    // Process manual maps: absolute ones are projected as-is (no translation offset);
    // relative ones are placed afterwards from their markers already placed by name.
    for manual in manual_maps.iter().filter(|m| m.coordinate_system != crate::schema::CoordinateSystem::Relative) {
        for loc in &manual.locations {
            let (Some(lat), Some(lon)) = (loc.real_lat, loc.real_lon) else {
                warnings.push(format!("Manual marker '{}' has no lat/lon; not placed", loc.name));
                continue;
            };
            all_fixed_locations.push((loc.name.clone(), lat, lon, loc.icon_type));
        }
        for tile in &manual.tiles {
            if let (Some(t_lat), Some(t_lon)) = (tile.real_lat, tile.real_lon) {
                all_fixed_tiles.push((tile.tile_type.clone(), t_lat, t_lon, priority_weight(manual.priority)));
            }
        }
    }
    for manual in manual_maps.iter().filter(|m| m.coordinate_system == crate::schema::CoordinateSystem::Relative) {
        let placed: HashMap<String, (f32, f32)> = all_fixed_locations.iter()
            .map(|(n, lat, lon, _)| (n.to_lowercase(), (*lat, *lon))).collect();
        let Some(transform) = relative_placement(manual, &placed, &mut warnings) else { continue };
        for loc in &manual.locations {
            if placed.contains_key(&loc.name.to_lowercase()) { continue; } // anchors are already placed
            let (lat, lon) = transform(loc.x, loc.y);
            all_fixed_locations.push((loc.name.clone(), lat, lon, loc.icon_type));
        }
        for tile in &manual.tiles {
            let (lat, lon) = transform(tile.x as f32, tile.y as f32);
            all_fixed_tiles.push((tile.tile_type.clone(), lat, lon, priority_weight(manual.priority)));
        }
    }

    // Now convert these absolute (Lat, Lon) points into the final grid
    let mut grid_locations = Vec::new();

    for (name, lat, lon, icon) in all_fixed_locations {
        let (gx, gy) = to_grid(lat, lon, resolution);
        grid_locations.push(FixedLocation {
            name, x: gx, y: gy, icon_type: icon,
            real_lat: Some(lat), real_lon: Some(lon),
            anchor: None, // We don't need to propagate anchors in the final grid
        });
    }

    // Downsample: each output cell takes the majority type of the finer cells landing in it
    // (ties broken by name, so the result does not depend on input order).
    let mut votes: HashMap<(i32, i32), HashMap<String, f32>> = HashMap::new();
    for (t_type, lat, lon, w) in all_fixed_tiles {
        if t_type == "unknown" { continue; } // no data: must not outvote data from other sources
        let (gx, gy) = to_grid(lat, lon, resolution);
        *votes.entry((gx.round() as i32, gy.round() as i32)).or_default().entry(t_type).or_insert(0.0) += w;
    }
    let grid_tiles_map: HashMap<(i32, i32), String> = votes.into_iter().map(|(k, v)| {
        let best = v.into_iter().max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0))).unwrap().0;
        (k, best)
    }).collect();

    // Grid bounds: every placed tile and every marker (a marker off the grid would be lost), padded.
    let xs = grid_tiles_map.keys().map(|k| k.0).chain(grid_locations.iter().map(|l| l.x.round() as i32));
    let ys = grid_tiles_map.keys().map(|k| k.1).chain(grid_locations.iter().map(|l| l.y.round() as i32));
    let (min_x, max_x) = xs.fold((i32::MAX, i32::MIN), |(a, b), v| (a.min(v), b.max(v)));
    let (min_y, max_y) = ys.fold((i32::MAX, i32::MIN), |(a, b), v| (a.min(v), b.max(v)));
    let (min_x, max_x, min_y, max_y) = if min_x > max_x { (-2, 12, -2, 12) } else { (min_x - 2, max_x + 2, min_y - 2, max_y + 2) };

    let width = (max_x - min_x + 1) as u32;
    let height = (max_y - min_y + 1) as u32;

    let mut final_tiles = Vec::new();
    // Fill background with "unknown" or "Water" depending on context? 
    // User image shows massive purple. Let's fill the bounding box of clusters, but keep it tight.
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let t_type = grid_tiles_map.get(&(x, y)).cloned().unwrap_or_else(|| "unknown".to_string());
            let (lat, lon) = from_grid(x as f32, y as f32, resolution);
            final_tiles.push(FixedTile {
                x: x - min_x,
                y: y - min_y,
                region: tile_region(&t_type, lat, lon),
                tile_type: t_type,
                real_lat: None, real_lon: None,
                anchor: None,
            });
        }
    }

    let final_locations = grid_locations.into_iter().map(|mut l| {
        l.x -= min_x as f32;
        l.y -= min_y as f32;
        l
    }).collect();

    FixedMapData {
        tiles: final_tiles,
        locations: final_locations,
        width, height,
        global_offset_x: 0, global_offset_y: 0,
        warnings,
        world_bounds: game_clusters.first().and_then(|m| m.world_bounds),
    }
}

/// Specialized fix that merges a cluster into a single RawMapData.
/// This is used for intermediate grouping of maps (e.g. FO1 + FO2).
pub fn fix_to_raw(raw_maps: Vec<RawMapData>) -> RawMapData {
    info!("Merging cluster with {} maps into raw data...", raw_maps.len());
    
    // We'll use a high-resolution intermediate fix (1.0 miles/hex)
    let resolution = 1.0;
    // A merged cluster keeps the best (lowest) priority tier among its sources.
    let priority = raw_maps.iter().map(|m| m.priority.max(1)).min().unwrap_or(1);
    let fixed = fix_with_terrain_inverse_warping(raw_maps, resolution, None);
    

    // We need to know the min/max indices used to derive current x,y
    // Since fix_with_terrain_inverse_warping doesn't return them, we have a problem.
    // However, if we store the reference lat/lon in FixedMapData, we can reconstruct.
    
    // Let's actually have fix_with_terrain_inverse_warping return a more detailed structure
    // or just calculate the real-world coords inside it and store them in FixedLocation.
    
    // For now, let's assume we can't easily change FixedLocation (it's for final output).
    // Let's just merge them as RawMapData but with absolute coordinates ALREADY set.
    
    // Wait, if I just want to merge FO1 and FO2, I can just rubbersheet them and return a merged RawMapData.
    
    RawMapData {
        locations: fixed.locations.into_iter().map(|l| crate::schema::RawLocation {
            name: l.name,
            x: l.x,
            y: l.y,
            icon_type: l.icon_type,
            real_lat: l.real_lat,
            real_lon: l.real_lon,
            terrain_override: None,
            anchor: l.anchor,
        }).collect(),
        tiles: fixed.tiles.into_iter().map(|t| crate::schema::RawTile {
            x: t.x,
            y: t.y,
            tile_type: t.tile_type,
            real_lat: t.real_lat,
            real_lon: t.real_lon,
            weight: 1.0,
            anchor: t.anchor,
        }).collect(),
        width: fixed.width,
        height: fixed.height,
        coordinate_system: crate::schema::CoordinateSystem::Absolute,
        coordinate_scale: 1.0,
        rows: None,
        has_grid: true,
        reference_lon: None,
        warnings: fixed.warnings, // Propagate warnings!
        terrain_data: None,
        world_bounds: fixed.world_bounds,
        miles_per_unit: None,
        priority,
        control_points: Vec::new(),
    }
}

fn fix_legacy_pass_through(raw_maps: Vec<RawMapData>) -> FixedMapData {
    // Original simple path
    let mut fixed_tiles = Vec::new();
    let mut fixed_locations = Vec::new();
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for map in &raw_maps {
        for tile in &map.tiles {
            fixed_tiles.push(FixedTile {
                x: tile.x,
                y: tile.y,
                tile_type: tile.tile_type.clone(),
                real_lat: tile.real_lat,
                real_lon: tile.real_lon,
                anchor: tile.anchor.clone(),
                region: None,
            });
            if (tile.x as i32) < min_x { min_x = tile.x as i32; }
            if (tile.y as i32) < min_y { min_y = tile.y as i32; }
            if (tile.x as i32) > max_x { max_x = tile.x as i32; }
            if (tile.y as i32) > max_y { max_y = tile.y as i32; }
        }
        let current_map_locations: Vec<FixedLocation> = map.locations.iter().map(|loc| {
            FixedLocation {
                name: loc.name.clone(),
                x: loc.x,
                y: loc.y,
                icon_type: loc.icon_type,
                real_lat: loc.real_lat,
                real_lon: loc.real_lon,
                anchor: loc.anchor.clone(),
            }
        }).collect();
        fixed_locations.extend(current_map_locations);
    }
    let width = if max_x >= min_x { (max_x - min_x + 1) as u32 } else { 10 };
    let height = if max_y >= min_y { (max_y - min_y + 1) as u32 } else { 10 };
    
    FixedMapData {
        tiles: fixed_tiles,
        locations: fixed_locations,
        width,
        height,
        global_offset_x: if min_x == i32::MAX { 0 } else { min_x },
        global_offset_y: if min_y == i32::MAX { 0 } else { min_y },
        warnings: raw_maps.iter().flat_map(|m| m.warnings.clone()).collect(),
        world_bounds: raw_maps.first().and_then(|m| m.world_bounds),
    }
}


// --- INVERSE WARPING PIPELINE ---
// struct MapGroup {
//     pub locations: Vec<RawMapData>, // Just the data parts? No, full map.
//     pub combined_locations: Vec<FixedLocation>,
//     pub combined_tiles: Vec<(i32, i32, String)>,
//     pub control_points: Vec<ControlPoint>,
//     pub id: usize,
// }

fn fix_with_terrain_inverse_warping(raw_maps: Vec<RawMapData>, miles_per_hex: f32, bounds_limit: Option<BoundingBox>) -> FixedMapData {
    if raw_maps.is_empty() { return FixedMapData { tiles: vec![], locations: vec![], width: 0, height: 0, global_offset_x: 0, global_offset_y: 0, warnings: vec![], world_bounds: None }; }
    
    let mut warnings = Vec::new();


    // 0. Build a Geographic Tile Map for all tiles that ALREADY have lat/lon (Manual data)
    // 0. Build a Manual Points Bin for sub-grid interpolation
    // We use a fixed ~6-mile grid for binning to ensure consistent lookup regardless of rendering resolution
    
    
    #[derive(Clone)]
    struct ManualPoint {
        lat: f32,
        lon: f32,
        tile_type: String,
        weight: f32,
    }
    
    // Map Real World Coords for lookup (parse coordinates or geocode addresses)
    // Key: lowercase name, Value: (lat, lon, canonical_name)
    let mut real_coords_map: HashMap<String, (f32, f32, String)> = HashMap::new();
    for (name, coords_str) in REAL_COORDS.iter() {
        if let Ok((lat, lon)) = geocoding::parse_coords(coords_str) {
            let canonical = name.to_string();
            real_coords_map.insert(canonical.to_lowercase(), (lat, lon, canonical.clone()));
            
            // FO4 editor-ID aliases (FO1/FO2 aliases live in the Interplay extractor, per game).
            // ponytail: belongs in the Bethesda extractor's name remap; move when FO4 is audited.
            if name.to_lowercase() == "diamond city" {
                real_coords_map.insert("diamondcity".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("diamond city".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("diamondcityref".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("diamondcitymarketref".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "goodneighbor" || name.to_lowercase() == "good neighbor" {
                real_coords_map.insert("goodneighbor".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("good neighbor".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("goodneighborref".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "bunker hill" {
                real_coords_map.insert("bunkerhillmonument".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("bunker hill".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("bunkerhillmonumentref".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "uss constitution" {
                real_coords_map.insert("ussconstitution".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("uss constitution".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("ussconstitutionref".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "boston public library" {
                real_coords_map.insert("bostonpubliclibrary".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("boston public library".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("bostonpubliclibraryref".to_string(), (lat, lon, canonical.clone()));
            }
        }
    }

    let mut manual_points_bins: HashMap<(i32, i32), Vec<ManualPoint>> = HashMap::new();
    let mut manual_projected_lats = Vec::new();
    let mut manual_projected_lons = Vec::new();
    let mut all_real_world_locs: Vec<(String, f32, f32, crate::schema::IconType, Option<String>)> = Vec::new();
    let mut seen_locations: HashSet<String> = HashSet::new();

    // 1. Process Absolute manual maps (already have lat/lon)
    for map in &raw_maps {
        if map.coordinate_system == crate::schema::CoordinateSystem::Absolute && map.coordinate_scale <= 1.01 {
            for tile in &map.tiles {
                if let (Some(lat), Some(lon)) = (tile.real_lat, tile.real_lon) {
                    let (bin_y, bin_x) = bin_of(lat, lon);
                    
                    manual_points_bins.entry((bin_y, bin_x)).or_default().push(ManualPoint {
                        lat, lon, tile_type: tile.tile_type.clone(),
                        weight: 2.0 * priority_weight(map.priority),
                    });
                    manual_projected_lats.push(lat);
                    manual_projected_lons.push(lon);
                }
            }

            for loc in &map.locations {
                if let (Some(lat), Some(lon)) = (loc.real_lat, loc.real_lon) {
                    let name_lower = loc.name.to_lowercase();
                    if seen_locations.contains(&name_lower) { continue; }
                    seen_locations.insert(name_lower);
                    all_real_world_locs.push((loc.name.clone(), lat, lon, loc.icon_type, loc.terrain_override.clone()));
                }
            }
        }
    }

    // Build independent RubberSheets and tile maps for Interplay games (coordinate_scale > 1.0)
    // Per marker name: (display name, icon, terrain override, [(game, lat, lon)]) in first-seen order.
    let mut game_placements: HashMap<String, (String, crate::schema::IconType, Option<String>, Vec<(usize, f32, f32, f32)>)> = HashMap::new();
    let mut placement_order: Vec<String> = Vec::new();
    let mut game_sheets: Vec<Option<(RubberSheet, f32)>> = Vec::new();
    let mut game_tile_maps: Vec<HashMap<(i32, i32), (String, f32)>> = Vec::new();

    for (game_idx, map) in raw_maps.iter().enumerate() {
        // Skip manual data in the rubbersheeting loop
        if map.coordinate_scale <= 1.01 { 
            game_sheets.push(None);
            game_tile_maps.push(HashMap::new());
            continue; 
        }

        let mut game_coords: HashMap<String, (f32, f32)> = HashMap::new();
        for loc in &map.locations {
            game_coords.insert(loc.name.to_lowercase(), (loc.x, loc.y));
        }

        // 1. Build control points strictly from hardcoded anchors
        let mut control_points = Vec::new();
        for (name_lower, (lat, lon, _canonical)) in &real_coords_map {
            if let Some((gx, gy)) = game_coords.get(name_lower) {
                control_points.push(ControlPoint {
                    name: name_lower.clone(),
                    game_x: *gx,
                    game_y: *gy,
                    real_lat: *lat,
                    real_lon: *lon,
                });
            }
        }

        // Extractor-derived anchors (e.g. drawn state borders).
        control_points.extend(map.control_points.iter().map(|c| ControlPoint {
            name: c.name.clone(), game_x: c.x, game_y: c.y, real_lat: c.lat, real_lon: c.lon,
        }));
        // Deterministic order (HashMap iteration is not) and no duplicate game positions.
        control_points.sort_by(|a, b| a.name.cmp(&b.name));
        control_points.dedup_by(|a, b| a.game_x == b.game_x && a.game_y == b.game_y);
        info!("Game #{} has {} control points: {:?}", game_idx, control_points.len(), control_points.iter().map(|c| &c.name).collect::<Vec<_>>());

        let sheet_opt = if control_points.len() >= 3 {
            let s = map.coordinate_scale;
            let area = (0.0, 0.0, map.width as f32 * s, map.height as f32 * s, s / 2.0);
            let (sheet, report) = RubberSheet::new_fold_free(control_points, area);
            for msg in report {
                let msg = format!("Game #{}: {}", game_idx, msg);
                warn!("{}", msg);
                warnings.push(msg);
            }

            // Anchors go through the warp too, so every marker sits on the terrain it came from.
            for loc in &map.locations {
                if loc.real_lat.is_some() && loc.real_lon.is_some() { continue; } // Handle manual markers later
                let (lat, lon) = sheet.transform(loc.x, loc.y);
                let entry = game_placements.entry(loc.name.to_lowercase()).or_insert_with(|| {
                    placement_order.push(loc.name.to_lowercase());
                    (loc.name.clone(), loc.icon_type, loc.terrain_override.clone(), Vec::new())
                });
                entry.3.push((game_idx, lat, lon, priority_weight(map.priority)));
            }
            Some((sheet, map.coordinate_scale))
        } else {
            None
        };

        game_sheets.push(sheet_opt);

        let mut tile_map: HashMap<(i32, i32), (String, f32)> = HashMap::new();
        for tile in &map.tiles {
            // Speed up: intern common types to avoid excessive allocations
            let interned_type = match tile.tile_type.as_str() {
                "Grass" => "Grass",
                "Water" => "Water",
                "Sand" => "Sand",
                "Rock" => "Rock",
                "City" => "City",
                "Forest" => "Forest",
                "Mountain" => "Mountain",
                "Misc" => "Misc",
                _ => &tile.tile_type,
            };
            tile_map.insert((tile.x, tile.y), (interned_type.to_string(), tile.weight * priority_weight(map.priority)));
        }
        game_tile_maps.push(tile_map);
    }

    // Merge rule: a place found in several games (or twice in one) gets the priority-weighted mean position;
    // disagreement beyond 1 mi is reported, never silently dropped.
    for key in &placement_order {
        if seen_locations.contains(key) { continue; } // manual absolute data takes precedence
        seen_locations.insert(key.clone());
        let (name, icon, terrain, ps) = &game_placements[key];
        let wsum = ps.iter().map(|p| p.3).sum::<f32>();
        let lat = ps.iter().map(|p| p.1 * p.3).sum::<f32>() / wsum;
        let lon = ps.iter().map(|p| p.2 * p.3).sum::<f32>() / wsum;
        let spread = ps.iter().map(|p| {
            let dy = (p.1 - lat) * MILES_PER_DEGREE_LAT;
            let dx = (p.2 - lon) * MILES_PER_DEGREE_LAT * lat.to_radians().cos();
            (dx * dx + dy * dy).sqrt()
        }).fold(0.0, f32::max);
        if spread > 1.0 {
            let msg = format!("Games disagree on '{}' by up to {:.1} mi from the merged position: {:?}", name, spread,
                ps.iter().map(|p| format!("game #{} ({:.3}, {:.3})", p.0, p.1, p.2)).collect::<Vec<_>>());
            warn!("{}", msg);
            warnings.push(msg);
        }
        all_real_world_locs.push((name.clone(), lat, lon, *icon, terrain.clone()));
    }

    // 3. Resolve Relative manual data (anchored to ANY resolved markers)
    for map in &raw_maps {
        if map.coordinate_system == crate::schema::CoordinateSystem::Relative {
            // Build temporary lookup from everything resolved so far
            let mut anchor_lookup: HashMap<String, (f32, f32)> = HashMap::new();
            for (name, lat, lon, ..) in &all_real_world_locs {
                anchor_lookup.insert(name.to_lowercase(), (*lat, *lon));
            }

            let Some(transform) = relative_placement(map, &anchor_lookup, &mut warnings) else { continue };

            // Transform markers
            for loc in &map.locations {
                let name_lower = loc.name.to_lowercase();
                if seen_locations.contains(&name_lower) { continue; }
                seen_locations.insert(name_lower);

                let (lat, lon) = transform(loc.x, loc.y);
                all_real_world_locs.push((loc.name.clone(), lat, lon, loc.icon_type, loc.terrain_override.clone()));
            }

            // Transform tiles
            for tile in &map.tiles {
                let (lat, lon) = transform(tile.x as f32, tile.y as f32);
                let (bin_y, bin_x) = bin_of(lat, lon);
                
                manual_points_bins.entry((bin_y, bin_x)).or_default().push(ManualPoint {
                    lat, lon, tile_type: tile.tile_type.clone(),
                    weight: 2.0 * priority_weight(map.priority),
                });
                manual_projected_lats.push(lat);
                manual_projected_lons.push(lon);
            }
        }
    }

    // Add terrain overrides as high-weight manual points
    for (_, lat, lon, _, terrain_opt) in &all_real_world_locs {
        if let Some(terrain) = terrain_opt {
            let (bin_y, bin_x) = bin_of(*lat, *lon);
            
                manual_points_bins.entry((bin_y, bin_x)).or_default().push(ManualPoint {
                    lat: *lat, 
                    lon: *lon, 
                    tile_type: terrain.clone(),
                    weight: 3.0 * priority_weight(1), // location terrain overrides come from tier-1 manual data
                });
        }
    }

    // 3. Define OUTPUT GRID Bounds
    let (min_lat_raw, max_lat_raw, min_lon_raw, max_lon_raw) = if let Some(b) = bounds_limit {
        (b.min_lat, b.max_lat, b.min_lon, b.max_lon)
    } else {
        let mut projected_lats = Vec::new();
        let mut projected_lons = Vec::new();

        for (game_idx, sheet_opt) in game_sheets.iter().enumerate() {
            if let Some((sheet, terrain_scale)) = sheet_opt {
                let tiles = &raw_maps[game_idx].tiles;
                if !tiles.is_empty() {
                    let min_tx = tiles.iter().map(|t| t.x).min().unwrap() as f32;
                    let max_tx = tiles.iter().map(|t| t.x).max().unwrap() as f32;
                    let min_ty = tiles.iter().map(|t| t.y).min().unwrap() as f32;
                    let max_ty = tiles.iter().map(|t| t.y).max().unwrap() as f32;
                    
                    let corners = [
                        (min_tx * (*terrain_scale), min_ty * (*terrain_scale)),
                        (max_tx * (*terrain_scale), min_ty * (*terrain_scale)),
                        (min_tx * (*terrain_scale), max_ty * (*terrain_scale)),
                        (max_tx * (*terrain_scale), max_ty * (*terrain_scale)),
                    ];
                    
                    for (gx, gy) in corners {
                        let (lat, lon) = sheet.transform(gx, gy);
                        projected_lats.push(lat);
                        projected_lons.push(lon);
                    }
                }
            }
        }
        
        projected_lats.extend(manual_projected_lats);
        projected_lons.extend(manual_projected_lons);
        projected_lats.extend(all_real_world_locs.iter().map(|(_, lat, ..)| *lat));
        projected_lons.extend(all_real_world_locs.iter().map(|(_, _, lon, ..)| *lon));

        if projected_lats.is_empty() {
            warn!("No markers found for fixing! Returning empty map.");
            return FixedMapData {
                tiles: vec![],
                locations: vec![],
                width: 0, height: 0,
                global_offset_x: 0, global_offset_y: 0,
                warnings: warnings,
                world_bounds: None,
                    };
        }

        let buffer = 0.2;
        let min_lat = projected_lats.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
        let max_lat = projected_lats.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
        let min_lon = projected_lons.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
        let max_lon = projected_lons.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
        (min_lat, max_lat, min_lon, max_lon)
    };

    if min_lat_raw == 0.0 && max_lat_raw == 0.0 {
         return FixedMapData {
            tiles: vec![],
            locations: vec![],
            width: 0, height: 0,
            global_offset_x: 0, global_offset_y: 0,
            warnings: warnings,
            world_bounds: None,
            };
    }

    // North-west and south-east corners on the shared grid.
    // Grid extent of the lat/lon box: on the conic projection the extreme columns/rows can come
    // from any corner, so check all four.
    let corners = [(max_lat_raw, min_lon_raw), (max_lat_raw, max_lon_raw), (min_lat_raw, min_lon_raw), (min_lat_raw, max_lon_raw)]
        .map(|(la, lo)| to_grid(la, lo, miles_per_hex));
    let min_col = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min).floor() as i32;
    let max_col = corners.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let min_row = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min).floor() as i32;
    let max_row = corners.iter().map(|c| c.1).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;

    let grid_height = (max_row - min_row + 1) as u32;
    let grid_width = (max_col - min_col + 1) as u32;

    let cell_latlon = |x: u32, y: u32| from_grid((min_col + x as i32) as f32, (min_row + y as i32) as f32, miles_per_hex);

    info!("Stable Grid ({:.1} mi/hex): {}x{} hexes anchored at {:?}",
             miles_per_hex, grid_width, grid_height, cell_latlon(0, 0));

    // 4. Inverse Sampling Loop (Parallelized)
    let mut fixed_tiles: Vec<FixedTile> = (0..grid_height).into_par_iter().flat_map(|y| {
        let mut row_tiles = Vec::new();
        for x in 0..grid_width {
            let (lat, lon) = cell_latlon(x, y);
            
            let mut type_votes: HashMap<String, f32> = HashMap::new();

            // A) Manual Data
            if !manual_points_bins.is_empty() {
                let (bin_y, bin_x) = bin_of(lat, lon);

                for by in (bin_y - 8)..=(bin_y + 8) {
                    for bx in (bin_x - 8)..=(bin_x + 8) {
                        if let Some(points) = manual_points_bins.get(&(by, bx)) {
                            for p in points {
                                // Distance in units of 6 mi.
                                let d_lat_game = (p.lat - lat) * MILES_PER_DEGREE_LAT / 6.0;
                                let d_lon_game = (p.lon - lon) * MILES_PER_DEGREE_LAT * lat.to_radians().cos() / 6.0;
                                let dist_sq = d_lat_game*d_lat_game + d_lon_game*d_lon_game;
                                if dist_sq < 4.0 {
                                    let weight = (1.0 / (dist_sq + 0.01)) * p.weight;
                                    *type_votes.entry(p.tile_type.clone()).or_insert(0.0) += weight;
                                }
                            }
                        }
                    }
                }
            }

            // B) Game Data
            for (game_idx, sheet_opt) in game_sheets.iter().enumerate() {
                if let Some((sheet, terrain_scale)) = sheet_opt {
                    if let Some((gx, gy)) = sheet.inverse_transform(lat, lon) {
                        // Each game contributes the square that contains this point: game squares
                        // are discrete, so no blending with neighbours (that erodes narrow features).
                        // Square i covers game units [i*s, (i+1)*s).
                        let sq = ((gx / terrain_scale).floor() as i32, (gy / terrain_scale).floor() as i32);
                        if let Some((tile_type, base_weight)) = game_tile_maps[game_idx].get(&sq) {
                            // Same strength a manual point has at its own location (1 / 0.01).
                            *type_votes.entry(tile_type.clone()).or_insert(0.0) += 100.0 * base_weight;
                        }
                    }
                }
            }
            
            type_votes.remove("unknown"); // no-data never counts as a vote, whatever its source
            let best_type = type_votes.iter()
                .max_by(|a, b| a.1.total_cmp(b.1).then_with(|| b.0.cmp(a.0))) // ties: by name, not hash order
                .map(|(t, _)| t.clone());
            
            row_tiles.push(FixedTile { 
                x: x as i32, 
                y: y as i32, 
                tile_type: best_type.unwrap_or_else(|| "unknown".to_string()),
                real_lat: Some(lat),
                real_lon: Some(lon),
                anchor: None, // Inverse warping doesn't have anchors per tile
                region: None, // assigned after cleanup and trimming
            });
        }
        row_tiles
    }).collect();

    let filled_count = fixed_tiles.iter().filter(|t| t.tile_type != "unknown").count();
    
    {
        // 1. Build Spatial Map for fast lookups
        let mut map_grid: HashMap<(i32, i32), (usize, String)> = HashMap::new();
        for (i, t) in fixed_tiles.iter().enumerate() {
            map_grid.insert((t.x, t.y), (i, t.tile_type.clone()));
        }

        let mut to_update: Vec<(usize, String)> = Vec::new();

        // Pass 1: Remove small Water groups (< 5)
        let mut visited_water: HashSet<(i32, i32)> = HashSet::new();
        // Identify all water tiles
        let water_keys: Vec<(i32, i32)> = map_grid.iter()
            .filter(|(_, (_, t))| t == "Water")
            .map(|(&k, _)| k)
            .collect();

        for start in water_keys {
            if visited_water.contains(&start) { continue; }
            
            // BFS
            let mut group_idxs = Vec::new();
            let mut boundary_votes: HashMap<String, i32> = HashMap::new();
            let mut queue = vec![start];
            visited_water.insert(start);
            
            if let Some((idx, _)) = map_grid.get(&start) { group_idxs.push(*idx); }

            let mut head = 0;
            while head < queue.len() {
                let (cx, cy) = queue[head];
                head += 1;

                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 { continue; }
                        let nx = cx + dx;
                        let ny = cy + dy;

                        if let Some((n_idx, n_type)) = map_grid.get(&(nx, ny)) {
                            if n_type == "Water" {
                                if !visited_water.contains(&(nx, ny)) {
                                    visited_water.insert((nx, ny));
                                    queue.push((nx, ny));
                                    group_idxs.push(*n_idx);
                                }
                            } else if n_type != "unknown" {
                                // Land neighbor -> Vote ("unknown" is no data, not land)
                                *boundary_votes.entry(n_type.clone()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }

            // Only replace when surrounded by known land (no neighbours = no evidence it is a speck).
            let best_land = boundary_votes.iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(t, _)| t.clone());
            if let (true, Some(best_land)) = (group_idxs.len() < 5, best_land) {
                for idx in group_idxs {
                    to_update.push((idx, best_land.clone()));
                }
            }
        }

        // Apply Pass 1 updates immediately so Pass 2 sees them?
        // Yes, otherwise a single land speck might be surrounded by small water specks we just removed.
        for (idx, new_type) in &to_update {
            fixed_tiles[*idx].tile_type = new_type.clone();
            // Update map_grid too for Pass 2
            let t = &fixed_tiles[*idx];
            map_grid.insert((t.x, t.y), (*idx, new_type.clone()));
        }
        to_update.clear();

        // Pass 2: Remove small Land specks (< 2)
        let mut visited_land: HashSet<(i32, i32)> = HashSet::new();
        // Identify all LAND tiles (not Water/Ocean, and not "unknown", which is no data)
        let land_keys: Vec<(i32, i32)> = map_grid.iter()
            .filter(|(_, (_, t))| t != "Water" && t != "unknown")
            .map(|(&k, _)| k)
            .collect();
            
        for start in land_keys {
            if visited_land.contains(&start) { continue; }

            // BFS
            let mut group_idxs = Vec::new();
            let mut boundary_votes: HashMap<String, i32> = HashMap::new(); // Likely mostly water
            let mut queue = vec![start];
            visited_land.insert(start);
            
            if let Some((idx, _)) = map_grid.get(&start) { group_idxs.push(*idx); }

            let mut head = 0;
            while head < queue.len() {
                let (cx, cy) = queue[head];
                head += 1;

                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 { continue; }
                        let nx = cx + dx;
                        let ny = cy + dy;

                        if let Some((n_idx, n_type)) = map_grid.get(&(nx, ny)) {
                            if n_type == "unknown" { continue; }
                            if n_type != "Water" {
                                if !visited_land.contains(&(nx, ny)) {
                                    visited_land.insert((nx, ny));
                                    queue.push((nx, ny));
                                    group_idxs.push(*n_idx);
                                }
                            } else {
                                // Water neighbor -> Vote
                                *boundary_votes.entry(n_type.clone()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }

            // Too small -> flip to the surrounding water, only if there is surrounding water.
            let best_water = boundary_votes.iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(t, _)| t.clone());
            if let (true, Some(best_water)) = (group_idxs.len() < 2, best_water) {
                for idx in group_idxs {
                    to_update.push((idx, best_water.clone()));
                }
            }
        }

        // Apply Pass 2
        for (idx, new_type) in to_update {
            fixed_tiles[idx].tile_type = new_type;
        }
    }
    
    info!("Filled {} tiles.", filled_count);

    // 5. Place Locations FIRST (on untrimmed grid)
    let mut all_locations_placed: Vec<FixedLocation> = Vec::new();
    
    for (name, lat, lon, icon_type, _) in all_real_world_locs.iter() {
        let (col, row) = to_grid(*lat, *lon, miles_per_hex);
        let grid_x = col - min_col as f32;
        let grid_y = row - min_row as f32;
        
        // Check for actual collision (markers overlapping, not just same tile)
        for existing_loc in &all_locations_placed {
            let dx = grid_x - existing_loc.x;
            let dy = grid_y - existing_loc.y;
            let distance = (dx * dx + dy * dy).sqrt();
            
            // Consider it a collision if markers are within 0.5 units
            if distance < 0.5 {
                let msg = format!("Marker collision: '{}' overlaps '{}' (distance: {:.2})", name, existing_loc.name, distance);
                warn!("WARNING: {}", msg);
                warnings.push(msg);
                break;
            }
        }
        
        all_locations_placed.push(FixedLocation {
            name: name.clone(),
            x: grid_x,
            y: grid_y,
            icon_type: *icon_type,
            real_lat: Some(*lat),
            real_lon: Some(*lon),
            anchor: None,
        });
    }

    info!("DEBUG: Placed {} markers. Terrain has {} tiles.", all_locations_placed.len(), fixed_tiles.len());

    // 6. NOW trim unknown tiles from edges
    let mut min_x_real = i32::MAX;
    let mut max_x_real = i32::MIN;
    let mut min_y_real = i32::MAX;
    let mut max_y_real = i32::MIN;
    
    // Use world_bounds if provided (usually for individual cluster extraction)
    if let Some(bounds) = raw_maps.first().and_then(|m| m.world_bounds) {
        min_x_real = bounds.min_x;
        max_x_real = bounds.max_x;
        min_y_real = bounds.min_y;
        max_y_real = bounds.max_y;
        info!("Using official world_bounds: ({}, {}) to ({}, {})", min_x_real, min_y_real, max_x_real, max_y_real);
    } else {
        for tile in &fixed_tiles {
            if tile.tile_type != "unknown" && tile.tile_type != "" {
                if tile.x < min_x_real { min_x_real = tile.x; }
                if tile.x > max_x_real { max_x_real = tile.x; }
                if tile.y < min_y_real { min_y_real = tile.y; }
                if tile.y > max_y_real { max_y_real = tile.y; }
            }
        }
    }

    // If no real tiles found, don't trim
    if min_x_real == i32::MAX {
        min_x_real = 0;
        max_x_real = grid_width as i32 - 1;
        min_y_real = 0;
        max_y_real = grid_height as i32 - 1;
    }

    // Add small buffer, but CLAMP to actual grid boundaries (0..grid_width)
    // to avoid creating new "unknown" borders on top/left
    let buffer_tiles = 1;
    min_x_real = (min_x_real - buffer_tiles).max(0);
    min_y_real = (min_y_real - buffer_tiles).max(0);
    max_x_real = (max_x_real + buffer_tiles).min(grid_width as i32 - 1);
    max_y_real = (max_y_real + buffer_tiles).min(grid_height as i32 - 1);
    
    let trimmed_width = (max_x_real - min_x_real + 1) as u32;
    let trimmed_height = (max_y_real - min_y_real + 1) as u32;
    
    info!("Trimming from {}x{} to {}x{} (offset: {}, {})", 
             grid_width, grid_height, trimmed_width, trimmed_height, min_x_real, min_y_real);
    
    // Adjust tile coordinates
    let trimmed_tiles: Vec<FixedTile> = fixed_tiles.into_iter()
        .filter(|t| t.x >= min_x_real && t.x <= max_x_real && t.y >= min_y_real && t.y <= max_y_real)
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|t| FixedTile {
            x: t.x - min_x_real,
            y: t.y - min_y_real,
            region: t.real_lat.zip(t.real_lon).and_then(|(la, lo)| tile_region(&t.tile_type, la, lo)),
            tile_type: t.tile_type,
            real_lat: t.real_lat,
            real_lon: t.real_lon,
            anchor: t.anchor,
        })
        .collect();

    // Adjust marker coordinates
    let trimmed_locations: Vec<FixedLocation> = all_locations_placed.into_iter()
        .map(|loc| FixedLocation {
            name: loc.name,
            x: loc.x - min_x_real as f32,
            y: loc.y - min_y_real as f32,
            icon_type: loc.icon_type,
            real_lat: loc.real_lat,
            real_lon: loc.real_lon,
            anchor: loc.anchor,
        })
        .collect();

    let mut final_warnings = Vec::new();
    for map in &raw_maps {
        final_warnings.extend(map.warnings.clone());
    }
    final_warnings.extend(warnings);

    FixedMapData {
        tiles: trimmed_tiles,
        locations: trimmed_locations,
        width: trimmed_width,
        height: trimmed_height,
        global_offset_x: 0,
        global_offset_y: 0,
        warnings: final_warnings,
        world_bounds: raw_maps.first().and_then(|m| m.world_bounds),
    }
}

#[cfg(test)]
mod invariant_tests {
    //! Logical-correctness invariants for the FO1/FO2 pipeline (see plan I1–I10).
    //! Reads the cached CITY.TXT/WORLDMAP.TXT under `.cache/` via the real extractor.
    use super::*;
    use crate::extractors::Extractor;
    use crate::extractors::interplay::{FalloutInterplayExtractor, GameVariant};

    fn game(v: GameVariant) -> RawMapData { FalloutInterplayExtractor::new(v).extract() }

    fn anchors() -> HashMap<String, (f32, f32)> {
        REAL_COORDS.iter()
            .map(|(n, c)| (n.to_lowercase(), geocoding::parse_coords(c).unwrap()))
            .collect()
    }

    fn control_points(map: &RawMapData, skip: Option<&str>) -> Vec<ControlPoint> {
        let a = anchors();
        map.locations.iter()
            .filter(|l| Some(l.name.to_lowercase().as_str()) != skip)
            .filter_map(|l| a.get(&l.name.to_lowercase()).map(|&(lat, lon)| ControlPoint {
                name: l.name.clone(), game_x: l.x, game_y: l.y, real_lat: lat, real_lon: lon,
            }))
            .collect()
    }

    /// Great-circle-free local distance in true miles (cos(lat) scaling, not the 35N constant).
    fn miles(a: (f32, f32), b: (f32, f32)) -> f32 {
        let lat_m = (a.0 + b.0) / 2.0;
        let dy = (a.0 - b.0) * MILES_PER_DEGREE_LAT;
        let dx = (a.1 - b.1) * MILES_PER_DEGREE_LAT * lat_m.to_radians().cos();
        (dx * dx + dy * dy).sqrt()
    }

    #[test]
    fn report_transform_quality() {
        for v in [GameVariant::Fallout1, GameVariant::Fallout2] {
            let map = game(v);
            // The world map is 28x30 squares of 50 px, whatever tile resolution the extractor emits.
            assert_eq!((map.width as f32 * map.coordinate_scale, map.height as f32 * map.coordinate_scale), (1400.0, 1500.0),
                "{:?}: world map must span 1400x1500 px", v);
            let cps = control_points(&map, None);
            let (sheet, report) = RubberSheet::new_fold_free(cps.clone(), (0.0, 0.0, 28.0 * 50.0, 30.0 * 50.0, 25.0));
            for line in report { println!("{:?} {}", v, line); }

            // I3: exact at anchors
            for cp in &cps {
                let p = sheet.transform(cp.game_x, cp.game_y);
                println!("{:?} anchor-exact {:<24} err {:.3} mi", v, cp.name, miles(p, (cp.real_lat, cp.real_lon)));
            }

            // I8: leave-one-out
            for cp in &cps {
                let loo = RubberSheet::new(control_points(&map, Some(&cp.name.to_lowercase())));
                let p = loo.transform(cp.game_x, cp.game_y);
                println!("{:?} LOO {:<24} err {:6.1} mi  predicted ({:.3}, {:.3}) anchor ({:.3}, {:.3})  dN {:+.1} mi dE {:+.1} mi",
                    v, cp.name, miles(p, (cp.real_lat, cp.real_lon)), p.0, p.1, cp.real_lat, cp.real_lon,
                    (p.0 - cp.real_lat) * MILES_PER_DEGREE_LAT,
                    (p.1 - cp.real_lon) * MILES_PER_DEGREE_LAT * cp.real_lat.to_radians().cos());
            }

            // I2: round trip forward→inverse over the whole world grid, in squares
            let mut worst_rt = 0f32;
            // I3: fold check — sign of Jacobian determinant over the grid
            let (mut pos, mut neg) = (0, 0);
            for sy in 0..30 { for sx in 0..28 {
                let (gx, gy) = ((sx as f32 + 0.5) * 50.0, (sy as f32 + 0.5) * 50.0);
                let p = sheet.transform(gx, gy);
                let (bx, by) = sheet.inverse_transform(p.0, p.1).unwrap();
                worst_rt = worst_rt.max(((bx - gx).powi(2) + (by - gy).powi(2)).sqrt() / 50.0);
                let h = 5.0;
                let px = sheet.transform(gx + h, gy);
                let py = sheet.transform(gx, gy + h);
                let det = (px.0 - p.0) * (py.1 - p.1) - (px.1 - p.1) * (py.0 - p.0);
                if det > 0.0 { pos += 1 } else { neg += 1; println!("{:?} fold at square ({}, {})", v, sx, sy) }
            }}
            println!("{:?} round-trip worst {:.2} squares; jacobian +{} / -{}", v, worst_rt, pos, neg);
            assert_eq!(neg, 0, "{:?}: warp folds (I3)", v);
            assert!(worst_rt < 0.01, "{:?}: inverse is not the forward's inverse (I2)", v);

            // I4: implied miles per square (affine scale), x and y separately
            let c = (14.0 * 50.0, 15.0 * 50.0);
            let o = sheet.transform(c.0, c.1);
            println!("{:?} miles/square ~ x {:.1}, y {:.1}", v,
                miles(o, sheet.transform(c.0 + 50.0, c.1)), miles(o, sheet.transform(c.0, c.1 + 50.0)));
        }
    }

    /// Markers and terrain come from the same warp, so each game marker must sit on the game
    /// square it was drawn on. Checks the warped tile under each marker against the source square.
    #[test]
    fn markers_sit_on_their_source_square() {
        let maps = vec![game(GameVariant::Fallout1), game(GameVariant::Fallout2)];
        let fixed = fix_with_terrain_inverse_warping(maps.clone(), 1.0, None);
        let tile_at: HashMap<(i32, i32), &str> = fixed.tiles.iter().map(|t| ((t.x, t.y), t.tile_type.as_str())).collect();
        let (mut ok, mut total) = (0, 0);
        for map in &maps {
            let src: HashMap<(i32, i32), &str> = map.tiles.iter().map(|t| ((t.x, t.y), t.tile_type.as_str())).collect();
            for loc in &map.locations {
                let s = map.coordinate_scale;
                let want = src[&((loc.x / s).floor() as i32, (loc.y / s).floor() as i32)];
                let Some(f) = fixed.locations.iter().find(|f| f.name == loc.name) else { continue };
                let got = tile_at.get(&(f.x.round() as i32, f.y.round() as i32)).copied().unwrap_or("none");
                total += 1;
                if got == want { ok += 1 } else { println!("{:<28} source {:<8} warped {}", loc.name, want, got) }
            }
        }
        println!("{}/{} markers on a tile of their source square's type", ok, total);
        assert!(ok * 10 >= total * 9, "markers drift off their terrain (I1/I2)");
    }

    /// Relative manual maps must survive the global (translation-only) pass: markers that are
    /// not anchors get placed from the anchors, never dropped.
    #[test]
    fn relative_manual_maps_are_placed_globally() {
        let mk = |name: &str, x: f32, y: f32, lat: Option<f32>, lon: Option<f32>| crate::schema::RawLocation {
            name: name.into(), x, y, icon_type: crate::schema::IconType::City,
            real_lat: lat, real_lon: lon, terrain_override: None, anchor: None,
        };
        let cluster = RawMapData {
            locations: vec![mk("A", 0.0, 0.0, Some(37.0), Some(-120.0)), mk("B", 0.0, 0.0, Some(38.0), Some(-120.0)),
                            mk("C", 0.0, 0.0, Some(37.0), Some(-119.0))],
            coordinate_scale: 1.0, ..Default::default()
        };
        let relative = RawMapData {
            locations: vec![mk("A", 0.0, 10.0, None, None), mk("B", 0.0, 0.0, None, None),
                            mk("C", 10.0, 10.0, None, None), mk("Mid", 5.0, 5.0, None, None)],
            coordinate_system: crate::schema::CoordinateSystem::Relative,
            width: 11, height: 11, coordinate_scale: 1.0, miles_per_unit: Some(6.0), ..Default::default()
        };
        let fixed = fix_translation_only(vec![cluster], vec![relative], 6.0, None);
        let mid = fixed.locations.iter().find(|l| l.name == "Mid").expect("non-anchor marker of a relative map was dropped");
        // 3 anchors = exact affine; Mid is the midpoint of B and C, i.e. (37.5, -119.5).
        assert!((mid.real_lat.unwrap() - 37.5).abs() < 1e-3 && (mid.real_lon.unwrap() + 119.5).abs() < 1e-3,
            "Mid placed at {:?}, {:?}", mid.real_lat, mid.real_lon);
    }

    /// Every real point the warp covers must invert (else the cell gets no terrain: a hole).
    /// Probes off-centre points on a fine grid, not just square centres.
    #[test]
    fn inverse_has_no_holes() {
        for v in [GameVariant::Fallout1, GameVariant::Fallout2] {
            let map = game(v);
            let (sheet, _) = RubberSheet::new_fold_free(control_points(&map, None), (0.0, 0.0, 1400.0, 1500.0, 25.0));
            let mut fails = 0;
            for gy in (3..1500).step_by(7) { for gx in (3..1400).step_by(7) {
                let p = sheet.transform(gx as f32, gy as f32);
                match sheet.inverse_transform(p.0, p.1) {
                    Some((bx, by)) if (bx - gx as f32).abs() < 0.5 && (by - gy as f32).abs() < 0.5 => {}
                    _ => fails += 1,
                }
            }}
            assert_eq!(fails, 0, "{:?}: {} points do not invert", v, fails);
        }
    }

    /// Votes are weighted by source priority: where a tier-1 and a tier-3 source disagree on a
    /// cell, the tier-1 type wins even when outnumbered (weights 16 vs 2 x 4).
    #[test]
    fn higher_priority_source_wins_the_vote() {
        let src = |priority: u8, tiles: Vec<(&str, f32)>| RawMapData {
            locations: vec![crate::schema::RawLocation { name: format!("anchor{}", priority), x: 0.0, y: 0.0,
                icon_type: crate::schema::IconType::Other, real_lat: Some(37.0), real_lon: Some(-118.0),
                terrain_override: None, anchor: None }],
            tiles: tiles.into_iter().map(|(t, dlon)| crate::schema::RawTile { x: 0, y: 0, tile_type: t.into(),
                real_lat: Some(37.0), real_lon: Some(-118.0 + dlon), weight: 1.0, anchor: None }).collect(),
            coordinate_scale: 1.0, priority, ..Default::default()
        };
        let fixed = fix_translation_only(
            vec![src(1, vec![("Sand", 0.0)]), src(3, vec![("Water", 0.0), ("Water", 0.001)])], vec![], 6.0, None);
        let land: Vec<&str> = fixed.tiles.iter().map(|t| t.tile_type.as_str()).filter(|t| *t != "unknown").collect();
        assert_eq!(land, vec!["Sand"]);
    }

    /// Map scale: distances measured on the hex grid (column step = 1 hex, row step = sqrt(3)/2)
    /// match real great-circle distances across the whole mapped region.
    #[test]
    fn grid_distances_match_real_distances() {
        let haversine = |a: (f32, f32), b: (f32, f32)| {
            let (la1, la2) = (a.0.to_radians() as f64, b.0.to_radians() as f64);
            let (dla, dlo) = (la2 - la1, (b.1 - a.1).to_radians() as f64);
            let h = (dla / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlo / 2.0).sin().powi(2);
            2.0 * 3958.8 * h.sqrt().asin()
        };
        let grid_miles = |a: (f32, f32), b: (f32, f32)| {
            let (pa, pb) = (to_grid(a.0, a.1, 6.0), to_grid(b.0, b.1, 6.0));
            (((pb.0 - pa.0) * 6.0).powi(2) + ((pb.1 - pa.1) * 6.0 * HEX_ROW_FRACTION).powi(2)).sqrt() as f64
        };
        let places = [("San Francisco", (37.79, -122.42)), ("Los Angeles", (34.05, -118.24)), ("Denver", (39.74, -104.99)),
                      ("Salt Lake City", (40.76, -111.89)), ("St. Louis", (38.63, -90.20)), ("Chicago", (41.88, -87.63))];
        for (i, (na, a)) in places.iter().enumerate() {
            for (nb, b) in &places[i + 1..] {
                let (real, map) = (haversine(*a, *b), grid_miles(*a, *b));
                let err = (map - real) / real;
                println!("{:<14} - {:<14} real {:6.0} mi  map {:6.0} mi  {:+.1}%", na, nb, real, map, err * 100.0);
                assert!(err.abs() < 0.01, "{} - {}: {:+.1}%", na, nb, err * 100.0);
            }
        }
    }
}
