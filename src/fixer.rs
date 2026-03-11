use crate::schema::{RawMapData, FixedMapData, FixedTile, FixedLocation};
use crate::rubbersheeting::{RubberSheet, ControlPoint};
use crate::geocoding;
use std::collections::{HashMap, HashSet};
use log::{info, warn};

// User-provided real-world coordinates
// Each entry can be either:
// - Numeric coordinates: ("Name", "37.376,-118.482")
// - Place name/address: ("Name", "Griffith Observatory, Los Angeles")
const FALLOUT1_REAL_COORDS: &[(&str, &str)] = &[
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
    ("DiamondCity", "42.346676,-71.097218"), // Fenway Park
    ("Goodneighbor", "42.358433,-71.057530"), // Old State House
    ("BostonPublicLibrary", "42.349395,-71.076615"),
    ("BunkerHillMonument", "42.376365,-71.060773"),
    ("USSConstitution", "42.372439,-71.056586"),
    ("OldNorthChurch", "42.366367,-71.054378"),
];

const MILES_PER_DEGREE_LAT: f32 = 69.0;
const MILES_PER_DEGREE_LON_AT_35N: f32 = 56.5;

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

/// New Global Aligner: Finds a single (dLat, dLon) offset per cluster based on an anchor.
pub fn fix_translation_only(game_clusters: Vec<RawMapData>, manual_maps: Vec<RawMapData>, resolution: f32, _bounds: Option<BoundingBox>) -> FixedMapData {
    info!("Aligning {} game clusters and {} manual maps via translation...", game_clusters.len(), manual_maps.len());
    
    let mut all_fixed_locations = Vec::new();
    let mut all_fixed_tiles = Vec::new();

    // We'll project everything to lat/lon first
    for cluster in game_clusters {
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
                
                for (name, coords_str) in FALLOUT1_REAL_COORDS {
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
                for (name, coords_str) in FALLOUT1_REAL_COORDS {
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
            for loc in cluster.locations {
                let lat = loc.real_lat.unwrap_or(loc.x); // x is fallback but shouldn't be needed
                let lon = loc.real_lon.unwrap_or(loc.y);
                all_fixed_locations.push((loc.name, lat + d_lat, lon + d_lon, loc.icon_type));
            }

            // Apply offset to all tiles
            for tile in cluster.tiles {
                if let (Some(t_lat), Some(t_lon)) = (tile.real_lat, tile.real_lon) {
                    all_fixed_tiles.push((tile.tile_type, t_lat + d_lat, t_lon + d_lon));
                }
            }
        }
    }

    // Process manual maps: simply project them (no translation offset)
    for manual in manual_maps {
        for loc in manual.locations {
            let lat = loc.real_lat.unwrap_or(loc.x);
            let lon = loc.real_lon.unwrap_or(loc.y);
            all_fixed_locations.push((loc.name, lat, lon, loc.icon_type));
        }
        for tile in manual.tiles {
            if let (Some(t_lat), Some(t_lon)) = (tile.real_lat, tile.real_lon) {
                all_fixed_tiles.push((tile.tile_type, t_lat, t_lon));
            } else {
                // If it's a relative manual tile without geocoding, we might have a problem.
                // But generally ManualExtractor geocodes the center.
            }
        }
    }

    // Now convert these absolute (Lat, Lon) points into the final 6.0 mi/hex grid
    // Reuse the stable grid logic from inverse_warping
    const UNIVERSE_ORIGIN_LAT: f32 = 50.0;
    const UNIVERSE_ORIGIN_LON: f32 = -130.0;
    let lat_step = resolution / MILES_PER_DEGREE_LAT;
    let lon_step = resolution / MILES_PER_DEGREE_LON_AT_35N;

    // Convert everything to grid indices
    let mut grid_locations = Vec::new();
    let mut grid_tiles_map: HashMap<(i32, i32), String> = HashMap::new();

    for (name, lat, lon, icon) in all_fixed_locations {
        let gy = ((UNIVERSE_ORIGIN_LAT - lat) / lat_step) as f32;
        let gx = ((lon - UNIVERSE_ORIGIN_LON) / lon_step) as f32;
        grid_locations.push(FixedLocation {
            name, x: gx, y: gy, icon_type: icon,
            real_lat: Some(lat), real_lon: Some(lon),
            anchor: None, // We don't need to propagate anchors in the final grid
        });
    }

    for (t_type, lat, lon) in all_fixed_tiles {
        let gy = ((UNIVERSE_ORIGIN_LAT - lat) / lat_step).round() as i32;
        let gx = ((lon - UNIVERSE_ORIGIN_LON) / lon_step).round() as i32;
        grid_tiles_map.insert((gx, gy), t_type);
    }

    // Determine grid bounds based on actual placed tiles, but with a small padding
    let min_x = grid_tiles_map.keys().map(|(x, _)| *x).min().unwrap_or(0) - 2;
    let max_x = grid_tiles_map.keys().map(|(x, _)| *x).max().unwrap_or(10) + 2;
    let min_y = grid_tiles_map.keys().map(|(_, y)| *y).min().unwrap_or(0) - 2;
    let max_y = grid_tiles_map.keys().map(|(_, y)| *y).max().unwrap_or(10) + 2;

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
        global_offset_x: 0, global_offset_y: 0
    }
}

/// Specialized fix that merges a cluster into a single RawMapData.
/// This is used for intermediate grouping of maps (e.g. FO1 + FO2).
pub fn fix_to_raw(raw_maps: Vec<RawMapData>) -> RawMapData {
    info!("Merging cluster with {} maps into raw data...", raw_maps.len());
    
    // We'll use a high-resolution intermediate fix (1.0 miles/hex)
    let resolution = 1.0;
    let fixed = fix_with_terrain_inverse_warping(raw_maps, resolution, None);
    
    let _lat_step = resolution / MILES_PER_DEGREE_LAT;
    let _lon_step = resolution / MILES_PER_DEGREE_LON_AT_35N;

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
        reference_lat: None,
        reference_lon: None,
        terrain_data: None,
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
    if raw_maps.is_empty() { return FixedMapData { tiles: vec![], locations: vec![], width: 0, height: 0, global_offset_x: 0, global_offset_y: 0 }; }

    // --- GLOBAL COORDINATE SYSTEM ---
    let universe_origin_lat = 50.0;
    let universe_origin_lon = -130.0;

    // 0. Build a Geographic Tile Map for all tiles that ALREADY have lat/lon (Manual data)
    // 0. Build a Manual Points Bin for sub-grid interpolation
    // We use a fixed ~6-mile grid for binning to ensure consistent lookup regardless of rendering resolution
    let lat_step = miles_per_hex / MILES_PER_DEGREE_LAT;
    let lon_step = miles_per_hex / MILES_PER_DEGREE_LON_AT_35N;
    
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
    for (name, coords_str) in FALLOUT1_REAL_COORDS.iter() {
        if let Ok((lat, lon)) = geocoding::parse_coords(coords_str) {
            let canonical = name.to_string();
            real_coords_map.insert(canonical.to_lowercase(), (lat, lon, canonical.clone()));
            
            // Add common aliases to ensure FO1 and FO2 find each other
            if name.to_lowercase() == "shady sands" {
                real_coords_map.insert("ncr".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("01_shady sands".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "mariposa military base" {
                real_coords_map.insert("military base".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("08_military base".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "the boneyard" {
                real_coords_map.insert("boneyard".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("10_boneyard".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "the cathedral" {
                real_coords_map.insert("cathedral".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("11_cathedral".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "diamondcity" {
                real_coords_map.insert("diamondcity".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("diamondcityref".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("diamondcitymarketref".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "goodneighbor" {
                real_coords_map.insert("goodneighbor".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("goodneighborref".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "bunkerhillmonument" {
                real_coords_map.insert("bunkerhillmonument".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("bunkerhillmonumentref".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "ussconstitution" {
                real_coords_map.insert("ussconstitution".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("ussconstitutionref".to_string(), (lat, lon, canonical.clone()));
            } else if name.to_lowercase() == "bostonpubliclibrary" {
                real_coords_map.insert("bostonpubliclibrary".to_string(), (lat, lon, canonical.clone()));
                real_coords_map.insert("bostonpubliclibraryref".to_string(), (lat, lon, canonical.clone()));
            }
        }
    }

    let mut manual_points_bins: HashMap<(i32, i32), Vec<ManualPoint>> = HashMap::new();
    let mut manual_projected_lats = Vec::new();
    let mut manual_projected_lons = Vec::new();
    let mut all_real_world_locs: Vec<(String, f32, f32, crate::schema::IconType, Option<String>)> = Vec::new();
    let mut seen_locations: HashSet<String> = HashSet::new();

    // 0. Pre-populate with ground truth anchors
    let mut ground_truth_added = HashSet::new();
    for (lower, (lat, lon, name)) in &real_coords_map {
        seen_locations.insert(lower.clone());
        
        if ground_truth_added.contains(name) { continue; }
        ground_truth_added.insert(name.clone());

        // Find icon type if it exists in any map
        let icon_type = raw_maps.iter()
            .flat_map(|m| &m.locations)
            .find(|l| l.name.eq_ignore_ascii_case(name))
            .map(|l| l.icon_type)
            .unwrap_or(crate::schema::IconType::Other);
            
        all_real_world_locs.push((name.clone(), *lat, *lon, icon_type, None));
    }

    // 1. Process Absolute manual maps (already have lat/lon)
    for map in &raw_maps {
        if map.coordinate_system == crate::schema::CoordinateSystem::Absolute && map.coordinate_scale <= 1.01 {
            for tile in &map.tiles {
                if let (Some(lat), Some(lon)) = (tile.real_lat, tile.real_lon) {
                    let bin_y = ((lat - universe_origin_lat) / bin_lat_step).round() as i32;
                    let bin_x = ((lon - universe_origin_lon) / bin_lon_step).round() as i32;
                    
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

        info!("Game #{} has {} hardcoded control points: {:?}", game_idx, control_points.len(), control_points.iter().map(|c| &c.name).collect::<Vec<_>>());
        info!("Game #{} has {} hardcoded control points", game_idx, control_points.len());
        
        let sheet_opt = if control_points.len() >= 3 {
            let sheet = RubberSheet::new(control_points);
            
            for loc in &map.locations {
                let name_lower = loc.name.to_lowercase();
                if loc.real_lat.is_some() && loc.real_lon.is_some() { continue; } // Handle manual markers later
                if seen_locations.contains(&name_lower) { continue; }
                seen_locations.insert(name_lower);
                
                let (lat, lon) = sheet.transform(loc.x, loc.y);
                all_real_world_locs.push((loc.name.clone(), lat, lon, loc.icon_type, loc.terrain_override.clone()));
            }
            Some((sheet, map.coordinate_scale))
        } else {
            None
        };

        game_sheets.push(sheet_opt);

        let mut tile_map = HashMap::new();
        for tile in &map.tiles {
            tile_map.insert((tile.x, tile.y), (tile.tile_type.clone(), tile.weight));
        }
        game_tile_maps.push(tile_map);
    }

    // 3. Resolve Relative manual data (anchored to ANY resolved markers)
    for map in &raw_maps {
        if map.coordinate_system == crate::schema::CoordinateSystem::Relative {
            // Build temporary lookup from everything resolved so far
            let mut anchor_lookup: HashMap<String, (f32, f32)> = HashMap::new();
            for (name, lat, lon, ..) in &all_real_world_locs {
                anchor_lookup.insert(name.to_lowercase(), (*lat, *lon));
            }

            let mut local_control_points = Vec::new();
            for loc in &map.locations {
                if let Some((lat, lon)) = anchor_lookup.get(&loc.name.to_lowercase()) {
                    local_control_points.push(ControlPoint {
                        name: loc.name.clone(),
                        game_x: loc.x,
                        game_y: loc.y,
                        real_lat: *lat,
                        real_lon: *lon,
                    });
                }
            }

            if local_control_points.is_empty() {
                warn!("Relative map has NO anchors among {} resolved markers, skipping", all_real_world_locs.len());
                continue;
            }

            info!("Resolving relative map with {} anchors", local_control_points.len());

            // Transformation function
            let transform = |gx: f32, gy: f32| -> (f32, f32) {
                match local_control_points.len() {
                    1 => {
                        // Translation only
                        let cp = &local_control_points[0];
                        (cp.real_lat + (gy - cp.game_y) * lat_step, // approximation
                         cp.real_lon + (gx - cp.game_x) * lon_step)
                    },
                    2 => {
                        // Translation + Uniform Scaling + Rotation (2-point affine/similarity)
                        let cp1 = &local_control_points[0];
                        let cp2 = &local_control_points[1];
                        
                        let dg_x = cp2.game_x - cp1.game_x;
                        let dg_y = cp2.game_y - cp1.game_y;
                        let dr_lat = cp2.real_lat - cp1.real_lat;
                        let dr_lon = cp2.real_lon - cp1.real_lon;
                        
                        let game_dist_sq = dg_x*dg_x + dg_y*dg_y;
                        if game_dist_sq < 1e-4 { return (cp1.real_lat, cp1.real_lon); }
                        
                        // complex number division/multiplication logic:
                        // (dr_lat + i*dr_lon) = (s * exp(i*theta)) * (dg_x + i*dg_y)
                        let s_cos = (dr_lat * dg_x + dr_lon * dg_y) / game_dist_sq;
                        let s_sin = (dr_lon * dg_x - dr_lat * dg_y) / game_dist_sq;
                        
                        let tx = gx - cp1.game_x;
                        let ty = gy - cp1.game_y;
                        
                        let res_lat = cp1.real_lat + (tx * s_cos - ty * s_sin);
                        let res_lon = cp1.real_lon + (tx * s_sin + ty * s_cos);
                        (res_lat, res_lon)
                    },
                    _ => {
                        // 3+ points: Use full Rubbersheet (warp)
                        let sheet = RubberSheet::new(local_control_points.clone());
                        sheet.transform(gx, gy)
                    }
                }
            };

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
                let bin_y = ((lat - universe_origin_lat) / bin_lat_step).round() as i32;
                let bin_x = ((lon - universe_origin_lon) / bin_lon_step).round() as i32;
                
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
            let bin_y = ((lat - universe_origin_lat) / bin_lat_step).round() as i32;
            let bin_x = ((lon - universe_origin_lon) / bin_lon_step).round() as i32;
            
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

        let buffer = 0.2;
        let min_lat = projected_lats.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
        let max_lat = projected_lats.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
        let min_lon = projected_lons.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
        let max_lon = projected_lons.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
        (min_lat, max_lat, min_lon, max_lon)
    };

    let min_lat_idx = ((min_lat_raw - universe_origin_lat) / lat_step).floor() as i32;
    let max_lat_idx = ((max_lat_raw - universe_origin_lat) / lat_step).ceil() as i32;
    let min_lon_idx = ((min_lon_raw - universe_origin_lon) / lon_step).floor() as i32;
    let max_lon_idx = ((max_lon_raw - universe_origin_lon) / lon_step).ceil() as i32;

    let grid_height = (max_lat_idx - min_lat_idx + 1).abs() as u32;
    let grid_width = (max_lon_idx - min_lon_idx + 1).abs() as u32;

    let get_lat = |idx: i32| universe_origin_lat + idx as f32 * lat_step;
    let get_lon = |idx: i32| universe_origin_lon + idx as f32 * lon_step;

    info!("Stable Grid ({:.1} mi/hex): {}x{} hexes anchored at lat={}, lon={}", 
             miles_per_hex, grid_width, grid_height, get_lat(max_lat_idx), get_lon(min_lon_idx));

    // 4. Inverse Sampling Loop (Parallelized)
    let mut fixed_tiles: Vec<FixedTile> = (0..grid_height).into_par_iter().flat_map(|y| {
        let current_lat_idx = max_lat_idx - y as i32;
        let lat = get_lat(current_lat_idx);
        
        let mut row_tiles = Vec::new();
        for x in 0..grid_width {
            let current_lon_idx = min_lon_idx + x as i32;
            let lon = get_lon(current_lon_idx);
            
            let mut type_votes: HashMap<String, f32> = HashMap::new();

            // A) Manual Data
            if !manual_points_bins.is_empty() {
                let bin_y = ((lat - universe_origin_lat) / bin_lat_step).round() as i32;
                let bin_x = ((lon - universe_origin_lon) / bin_lon_step).round() as i32;

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
                        if gx < -20.0 || gx > (28.0 * terrain_scale + 20.0) || 
                           gy < -20.0 || gy > (30.0 * terrain_scale + 20.0) {
                            continue;
                        }

                        let rx = gx / (*terrain_scale);
                        let ry = gy / (*terrain_scale);
                        let base_rx = rx.round() as i32;
                        let base_ry = ry.round() as i32;

                        for irx in (base_rx - 1)..=(base_rx + 1) {
                            for iry in (base_ry - 1)..=(base_ry + 1) {
                                if irx < 0 || iry < 0 || irx >= raw_maps[game_idx].width as i32 || iry >= raw_maps[game_idx].height as i32 {
                                    continue;
                                }

                                if let Some((tile_type, base_weight)) = game_tile_maps[game_idx].get(&(irx, iry)) {
                                    let dx = rx - irx as f32;
                                    let dy = ry - iry as f32;
                                    let dist_sq = dx*dx + dy*dy;
                                    if dist_sq < 4.0 {
                                        let weight = (1.0 / (dist_sq + 0.01)) * (*base_weight);
                                        *type_votes.entry(tile_type.clone()).or_insert(0.0) += weight;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            let best_type = type_votes.iter()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
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
                            } else {
                                // Land neighbor -> Vote
                                *boundary_votes.entry(n_type.clone()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }

            if group_idxs.len() < 5 {
                // Determine replacement
                let best_land = boundary_votes.iter()
                    .max_by_key(|&(_, c)| c)
                    .map(|(t, _)| t.clone())
                    .unwrap_or("Sand".to_string());
                
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
        // Identify all LAND tiles (not Water/Ocean)
        let land_keys: Vec<(i32, i32)> = map_grid.iter()
            .filter(|(_, (_, t))| t != "Water" && t != "ocean")
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

            if group_idxs.len() < 2 {
                // Too small -> Flip to Water
                let best_water = boundary_votes.iter()
                    .max_by_key(|&(_, c)| c)
                    .map(|(t, _)| t.clone())
                    .unwrap_or("Water".to_string()); // Default Water

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
        let f_lat_idx = (lat - universe_origin_lat) / lat_step;
        let f_lon_idx = (lon - universe_origin_lon) / lon_step;
        let grid_x = f_lon_idx - (min_lon_idx as f32);
        let grid_y = (max_lat_idx as f32) - f_lat_idx;
        
        // Check for actual collision (markers overlapping, not just same tile)
        for existing_loc in &all_locations_placed {
            let dx = grid_x - existing_loc.x;
            let dy = grid_y - existing_loc.y;
            let distance = (dx * dx + dy * dy).sqrt();
            
            // Consider it a collision if markers are within 0.5 units
            if distance < 0.5 {
                warn!("WARNING: Marker collision: '{}' overlaps '{}' (distance: {:.2})", 
                         name, existing_loc.name, distance);
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
    
    for tile in &fixed_tiles {
        if tile.tile_type != "unknown" {
            if tile.x < min_x_real { min_x_real = tile.x; }
            if tile.x > max_x_real { max_x_real = tile.x; }
            if tile.y < min_y_real { min_y_real = tile.y; }
            if tile.y > max_y_real { max_y_real = tile.y; }
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

    FixedMapData {
        tiles: trimmed_tiles,
        locations: trimmed_locations,
        width: trimmed_width,
        height: trimmed_height,
        global_offset_x: 0,
        global_offset_y: 0,
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


    for (name, coords_str) in FALLOUT1_REAL_COORDS {
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
    }
}
