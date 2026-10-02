use crate::schema::{RawMapData, FixedMapData, FixedTile, FixedLocation};
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
    // Fallout 4
    ("Diamond City", "42.346676,-71.097218"), // Fenway Park
    ("Goodneighbor", "42.358433,-71.057530"), // Old State House
    ("Boston Public Library", "42.349395,-71.076615"),
    ("Bunker Hill", "42.376365,-71.060773"),
    ("USS Constitution", "42.372439,-71.056586"),
    ("Old North Church", "42.366367,-71.054378"),
];

const MILES_PER_DEGREE_LAT: f32 = 69.0;
// ponytail: one longitude scale (35N) for the whole grid; east-west distances are off by up to
// ~8% at 33N/42N. Switch to a real projection when maps beyond California/Nevada need it.
const MILES_PER_DEGREE_LON_AT_35N: f32 = 56.5;

const UNIVERSE_ORIGIN_LAT: f32 = 50.0;
const UNIVERSE_ORIGIN_LON: f32 = -130.0;

/// The output-grid convention, used by every stage: continuous (col, row) of a lat/lon on the
/// world grid at `miles_per_hex`. Rows grow south; integer indices are cell centres.
/// (Each game's own coordinate scale is separate: see `RawMapData::coordinate_scale`.)
pub fn to_grid(lat: f32, lon: f32, miles_per_hex: f32) -> (f32, f32) {
    ((lon - UNIVERSE_ORIGIN_LON) / (miles_per_hex / MILES_PER_DEGREE_LON_AT_35N),
     (UNIVERSE_ORIGIN_LAT - lat) / (miles_per_hex / MILES_PER_DEGREE_LAT))
}

/// Inverse of `to_grid`: (lat, lon) at grid position (col, row).
pub fn from_grid(col: f32, row: f32, miles_per_hex: f32) -> (f32, f32) {
    (UNIVERSE_ORIGIN_LAT - row * (miles_per_hex / MILES_PER_DEGREE_LAT),
     UNIVERSE_ORIGIN_LON + col * (miles_per_hex / MILES_PER_DEGREE_LON_AT_35N))
}

/// Key of the 1-mile bin holding (lat, lon), for neighbour lookups of manual points.
fn bin_of(lat: f32, lon: f32) -> (i32, i32) {
    let (c, r) = to_grid(lat, lon, 1.0);
    (r.round() as i32, c.round() as i32)
}

// const LAT_STEP: f32 = MILES_PER_HEX / MILES_PER_DEGREE_LAT;
// const LON_STEP: f32 = MILES_PER_HEX / MILES_PER_DEGREE_LON_AT_35N;

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
                    all_fixed_tiles.push((tile.tile_type.clone(), t_lat + d_lat, t_lon + d_lon));
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
                all_fixed_tiles.push((tile.tile_type.clone(), t_lat, t_lon));
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
            all_fixed_tiles.push((tile.tile_type.clone(), lat, lon));
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
    let mut votes: HashMap<(i32, i32), HashMap<String, u32>> = HashMap::new();
    for (t_type, lat, lon) in all_fixed_tiles {
        if t_type == "unknown" { continue; } // no data: must not outvote data from other sources
        let (gx, gy) = to_grid(lat, lon, resolution);
        *votes.entry((gx.round() as i32, gy.round() as i32)).or_default().entry(t_type).or_insert(0) += 1;
    }
    let grid_tiles_map: HashMap<(i32, i32), String> = votes.into_iter().map(|(k, v)| {
        let best = v.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).unwrap().0;
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
            final_tiles.push(FixedTile {
                x: x - min_x,
                y: y - min_y,
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
    
    let bin_lat_step = 1.0 / MILES_PER_DEGREE_LAT;
    let bin_lon_step = 1.0 / MILES_PER_DEGREE_LON_AT_35N;
    
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
                        weight: 2.0,
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
    let mut game_placements: HashMap<String, (String, crate::schema::IconType, Option<String>, Vec<(usize, f32, f32)>)> = HashMap::new();
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
                entry.3.push((game_idx, lat, lon));
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
            tile_map.insert((tile.x, tile.y), (interned_type.to_string(), tile.weight));
        }
        game_tile_maps.push(tile_map);
    }

    // Merge rule: a place found in several games (or twice in one) gets the mean position;
    // disagreement beyond 1 mi is reported, never silently dropped.
    for key in &placement_order {
        if seen_locations.contains(key) { continue; } // manual absolute data takes precedence
        seen_locations.insert(key.clone());
        let (name, icon, terrain, ps) = &game_placements[key];
        let lat = ps.iter().map(|p| p.1).sum::<f32>() / ps.len() as f32;
        let lon = ps.iter().map(|p| p.2).sum::<f32>() / ps.len() as f32;
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
                    weight: 2.0,
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
                    weight: 3.0, // High weight for specific location terrain overrides
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
    let (nw_col, nw_row) = to_grid(max_lat_raw, min_lon_raw, miles_per_hex);
    let (se_col, se_row) = to_grid(min_lat_raw, max_lon_raw, miles_per_hex);
    let (min_col, min_row) = (nw_col.floor() as i32, nw_row.floor() as i32);
    let (max_col, max_row) = (se_col.ceil() as i32, se_row.ceil() as i32);

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
                                let d_lat_game = ((p.lat - lat) / bin_lat_step) / 6.0;
                                let d_lon_game = ((p.lon - lon) / bin_lon_step) / 6.0;
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
            .filter(|(_, (_, t))| t == "Water" || t == "ocean")
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
                            if n_type == "Water" || n_type == "ocean" {
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
            .filter(|(_, (_, t))| t != "Water" && t != "ocean" && t != "unknown")
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
                            if n_type != "Water" && n_type != "ocean" {
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
    // to avoid creating new "ocean" (unknown) borders on top/left
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
        .map(|t| FixedTile {
            x: t.x - min_x_real,
            y: t.y - min_y_real,
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

#[allow(dead_code)]
fn fix_with_rubbersheeting(raw_maps: Vec<RawMapData>) -> FixedMapData {
    // Step 1: Collect all locations from extractor (with game coordinates)
    let mut all_locations = Vec::new();
    for map in &raw_maps {
        all_locations.extend(map.locations.clone());
    }
    
    // Step 2: Build control points by matching names
    let mut control_points = Vec::new();
    let mut real_world_locations = Vec::new(); // For final placement
    
    // Create local game_coords map
    let mut game_coords: HashMap<String, (f32, f32)> = HashMap::new();
    for loc in &all_locations {
        game_coords.insert(loc.name.to_lowercase(), (loc.x, loc.y));
    }


    for (name, coords_str) in REAL_COORDS {
        if let Ok((lat, lon)) = geocoding::parse_coords(coords_str) {
            if let Some((gx, gy)) = game_coords.get(&name.to_lowercase()) {
                control_points.push(ControlPoint {
                    name: name.to_string(),
                    game_x: *gx,
                    game_y: *gy,
                    real_lat: lat,
                    real_lon: lon,
                });
                // Also store for looking up icon type later
                 if let Some(loc_data) = all_locations.iter().find(|l| l.name.eq_ignore_ascii_case(name)) {
                    real_world_locations.push((name.to_string(), lat, lon, loc_data.icon_type));
                }
            }
        }
    }
    
    info!("Built RubberSheet with {} control points", control_points.len());
    let _sheet = RubberSheet::new(control_points); // Unused in legacy mode for markers but kept for consistency

    // 3. Define OUTPUT GRID Bounds
    // We strictly use the bounds of the Real World Locations (plus small buffer)
    let buffer = 0.1; // 0.1 degree ~ 7 miles
    
    let all_lats: Vec<f32> = real_world_locations.iter().map(|(_, lat, _, _)| *lat).collect();
    let all_lons: Vec<f32> = real_world_locations.iter().map(|(_, _, lon, _)| *lon).collect();
    
    if all_lats.is_empty() { return fix_legacy_pass_through(raw_maps); }

    let min_lat = all_lats.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
    let max_lat = all_lats.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
    let min_lon = all_lons.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
    let max_lon = all_lons.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
    
    let lat_range = max_lat - min_lat;
    let lon_range = max_lon - min_lon;
    
    // Hex sizes in degrees
    // Hex sizes in degrees (Legacy constant 6.0)
    let hex_height_deg = 6.0 / MILES_PER_DEGREE_LAT; // ~0.087
    let hex_width_deg = 6.0 / MILES_PER_DEGREE_LON_AT_35N; // ~0.106
    
    let grid_height = (lat_range / hex_height_deg).ceil() as u32;
    let grid_width = (lon_range / hex_width_deg).ceil() as u32;
    
    info!("Inverse Warping Grid: {}x{} hexes (spanning {:.2} deg lat, {:.2} deg lon)", 
             grid_width, grid_height, lat_range, lon_range);

    // 4. Generate Placeholder Terrain (Sand)
    let mut fixed_tiles = Vec::new();
    
    for y in 0..grid_height {
        for x in 0..grid_width {
            // Check if within map bounds (optional, or just fill rectangle)
             fixed_tiles.push(FixedTile {
                x: x as i32,
                y: y as i32,
                tile_type: "Sand".to_string(),
                real_lat: None,
                real_lon: None,
                anchor: None,
            });
        }
    }
    
    info!("Generated {} Sand tiles for marker-only map", fixed_tiles.len());
    
    // 5. Place Locations
    let final_locations: Vec<FixedLocation> = real_world_locations.iter().map(|(name, lat, lon, icon_type)| {
        let u = (lon - min_lon) / lon_range;
        let v = (max_lat - lat) / lat_range; // Inverted Y logic matches loop
        
        FixedLocation {
            name: name.clone(),
            x: u * grid_width as f32,
            y: v * grid_height as f32,
            icon_type: *icon_type,
            real_lat: Some(*lat),
            real_lon: Some(*lon),
            anchor: None,
        }
    }).collect();

    FixedMapData {
        tiles: fixed_tiles,
        locations: final_locations,
        width: grid_width,
        height: grid_height,
        global_offset_x: 0,
        global_offset_y: 0,
        warnings: vec![],
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
            assert_eq!(map.coordinate_scale, 50.0, "{:?}: interplay scale must be 50 px/square", v);
            assert_eq!((map.width, map.height), (28, 30), "{:?}: world map must be 28x30 squares", v);
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
                let want = src[&((loc.x / 50.0).floor() as i32, (loc.y / 50.0).floor() as i32)];
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
}
