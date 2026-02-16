use crate::schema::{RawMapData, FixedMapData, FixedTile, FixedLocation};
use crate::rubbersheeting::{RubberSheet, ControlPoint};
use crate::geocoding;
use std::collections::HashMap;

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
];

const MILES_PER_HEX: f32 = 6.0;
const MILES_PER_DEGREE_LAT: f32 = 69.0;
const MILES_PER_DEGREE_LON_AT_35N: f32 = 56.5;

const LAT_STEP: f32 = MILES_PER_HEX / MILES_PER_DEGREE_LAT;
const LON_STEP: f32 = MILES_PER_HEX / MILES_PER_DEGREE_LON_AT_35N;

pub fn fix(raw_maps: Vec<RawMapData>) -> FixedMapData {
    println!("Running Fixer on {} raw maps...", raw_maps.len());

    // Check if we should apply rubbersheeting (Fallout 1 data with control points)
    let has_locations = raw_maps.iter().any(|m| !m.locations.is_empty());
    let has_tiles = raw_maps.iter().any(|m| !m.tiles.is_empty());
    
    // If we have both locations and terrain, use Inverse Warping
    if has_locations && has_tiles {
        println!("Invoking Inverse Warping pipeline for terrain + locations...");
        return fix_with_terrain_inverse_warping(raw_maps);
    }
    
    // Fallback logic for extraction without terrain
    if has_locations && !has_tiles {
        println!("No tiles provided - generating placeholder terrain");
        return fix_legacy_pass_through(raw_maps);
    }

    // Default pass through
    fix_legacy_pass_through(raw_maps)
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
struct MapGroup {
    pub locations: Vec<RawMapData>, // Just the data parts? No, full map.
    pub combined_locations: Vec<FixedLocation>,
    pub combined_tiles: Vec<(i32, i32, String)>,
    pub control_points: Vec<ControlPoint>,
    pub id: usize,
}

fn fix_with_terrain_inverse_warping(raw_maps: Vec<RawMapData>) -> FixedMapData {
    if raw_maps.is_empty() { return FixedMapData { tiles: vec![], locations: vec![], width: 0, height: 0, global_offset_x: 0, global_offset_y: 0 }; }

    // --- GLOBAL COORDINATE SYSTEM ---
    let universe_origin_lat = 50.0;
    let universe_origin_lon = -130.0;

    // 0. Build a Geographic Tile Map for all tiles that ALREADY have lat/lon (Manual data)
    let mut geo_tile_map: HashMap<(i32, i32), String> = HashMap::new();
    let mut manual_projected_lats = Vec::new();
    let mut manual_projected_lons = Vec::new();

    for map in &raw_maps {
        for tile in &map.tiles {
            if let (Some(lat), Some(lon)) = (tile.real_lat, tile.real_lon) {
                let lat_idx = ((lat - universe_origin_lat) / LAT_STEP).round() as i32;
                let lon_idx = ((lon - universe_origin_lon) / LON_STEP).round() as i32;
                geo_tile_map.insert((lat_idx, lon_idx), tile.tile_type.clone());
                manual_projected_lats.push(lat);
                manual_projected_lons.push(lon);
            }
        }
    }

    // Map Real World Coords for lookup (parse coordinates or geocode addresses)
    let mut real_coords_map: HashMap<String, (f32, f32)> = HashMap::new();
    for (name, coords_str) in FALLOUT1_REAL_COORDS.iter() {
        if let Ok((lat, lon)) = geocoding::parse_coords(coords_str) {
            real_coords_map.insert(name.to_lowercase(), (lat, lon));
            
            // Add common aliases to ensure FO1 and FO2 find each other
            if name.to_lowercase() == "shady sands" {
                real_coords_map.insert("ncr".to_string(), (lat, lon));
                real_coords_map.insert("01_shady sands".to_string(), (lat, lon));
            } else if name.to_lowercase() == "mariposa military base" {
                real_coords_map.insert("military base".to_string(), (lat, lon));
                real_coords_map.insert("08_military base".to_string(), (lat, lon));
            } else if name.to_lowercase() == "the boneyard" {
                real_coords_map.insert("boneyard".to_string(), (lat, lon));
                real_coords_map.insert("10_boneyard".to_string(), (lat, lon));
            } else if name.to_lowercase() == "the cathedral" {
                real_coords_map.insert("cathedral".to_string(), (lat, lon));
                real_coords_map.insert("11_cathedral".to_string(), (lat, lon));
            }
        }
    }

    // Build independent RubberSheets and tile maps for Interplay games (coordinate_scale > 1.0)
    let mut game_sheets: Vec<Option<(RubberSheet, f32)>> = Vec::new();
    let mut game_tile_maps: Vec<HashMap<(i32, i32), String>> = Vec::new();
    let mut all_real_world_locs: Vec<(String, f32, f32, crate::schema::IconType)> = Vec::new();
    let mut seen_locations: std::collections::HashSet<String> = std::collections::HashSet::new();

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
        for (name_lower, (lat, lon)) in &real_coords_map {
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

        println!("Game #{} has {} hardcoded control points", game_idx, control_points.len());

        let sheet_opt = if control_points.len() >= 3 {
            let sheet = RubberSheet::new(control_points);
            
            for loc in &map.locations {
                let name_lower = loc.name.to_lowercase();
                if loc.real_lat.is_some() && loc.real_lon.is_some() { continue; } // Handle manual markers later
                if seen_locations.contains(&name_lower) { continue; }
                seen_locations.insert(name_lower);
                
                let (lat, lon) = sheet.transform(loc.x, loc.y);
                all_real_world_locs.push((loc.name.clone(), lat, lon, loc.icon_type));
            }
            Some((sheet, map.coordinate_scale))
        } else {
            None
        };

        game_sheets.push(sheet_opt);

        let mut tile_map = HashMap::new();
        for tile in &map.tiles {
            tile_map.insert((tile.x, tile.y), tile.tile_type.clone());
        }
        game_tile_maps.push(tile_map);
    }

    // 2. Add Manual Markers (GPS accurate)
    for map in &raw_maps {
        for loc in &map.locations {
            if let (Some(lat), Some(lon)) = (loc.real_lat, loc.real_lon) {
                all_real_world_locs.push((loc.name.clone(), lat, lon, loc.icon_type));
            }
        }
    }

    // 3. Define OUTPUT GRID Bounds from TILE CORNERS (not just markers)
    let mut projected_lats = Vec::new();
    let mut projected_lons = Vec::new();

    for (game_idx, sheet_opt) in game_sheets.iter().enumerate() {
        if let Some((sheet, terrain_scale)) = sheet_opt {
            // Find tile bounds for this game
            let tiles = &raw_maps[game_idx].tiles;
            
            if !tiles.is_empty() {
                let min_tx = tiles.iter().map(|t| t.x).min().unwrap() as f32;
                let max_tx = tiles.iter().map(|t| t.x).max().unwrap() as f32;
                let min_ty = tiles.iter().map(|t| t.y).min().unwrap() as f32;
                let max_ty = tiles.iter().map(|t| t.y).max().unwrap() as f32;
                
                println!("Game #{} tile bounds: x=[{}, {}], y=[{}, {}], scale={}", game_idx, min_tx, max_tx, min_ty, max_ty, terrain_scale);
                
                // Project all 4 corners of this game's tile area
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
    
    // Include manual tile bounds
    projected_lats.extend(manual_projected_lats);
    projected_lons.extend(manual_projected_lons);
    
    // Also include marker locations to ensure they're visible
    projected_lats.extend(all_real_world_locs.iter().map(|(_, lat, _, _)| *lat));
    projected_lons.extend(all_real_world_locs.iter().map(|(_, _, lon, _)| *lon));

    if !projected_lats.is_empty() {
        let temp_min_lat = projected_lats.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let temp_max_lat = projected_lats.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let temp_min_lon = projected_lons.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let temp_max_lon = projected_lons.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        println!("Projected bounds: lat=[{:.2}, {:.2}], lon=[{:.2}, {:.2}]", temp_min_lat, temp_max_lat, temp_min_lon, temp_max_lon);
        println!("Range: {:.2} deg lat x {:.2} deg lon", temp_max_lat - temp_min_lat, temp_max_lon - temp_min_lon);
    }

    let buffer = 0.2;
    let min_lat_raw = projected_lats.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
    let max_lat_raw = projected_lats.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
    let min_lon_raw = projected_lons.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
    let max_lon_raw = projected_lons.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;


    // --- ABSOLUTE GRID ANCHORING ---
    // Instead of aligning to the current bounds (which shift when markers are added),
    // we align to an absolute origin in the universe (e.g. 50N, 130W).

    let min_lat_idx = ((min_lat_raw - universe_origin_lat) / LAT_STEP).floor() as i32;
    let max_lat_idx = ((max_lat_raw - universe_origin_lat) / LAT_STEP).ceil() as i32;
    let min_lon_idx = ((min_lon_raw - universe_origin_lon) / LON_STEP).floor() as i32;
    let max_lon_idx = ((max_lon_raw - universe_origin_lon) / LON_STEP).ceil() as i32;

    let grid_height = (max_lat_idx - min_lat_idx + 1).abs() as u32;
    let grid_width = (max_lon_idx - min_lon_idx + 1).abs() as u32;

    println!("Universe Grid: {}x{} hexes (lat_idx: {}..{}, lon_idx: {}..{})", 
             grid_width, grid_height, min_lat_idx, max_lat_idx, min_lon_idx, max_lon_idx);

    // Helper to get absolute lat/lon from absolute indices
    let get_lat = |idx: i32| universe_origin_lat + idx as f32 * LAT_STEP;
    let get_lon = |idx: i32| universe_origin_lon + idx as f32 * LON_STEP;

    println!("Stable Grid: {}x{} hexes anchored at lat={}, lon={}", 
             grid_width, grid_height, get_lat(max_lat_idx), get_lon(min_lon_idx));

    // 4. Inverse Sampling Loop (Composite)
    let mut fixed_tiles = Vec::new();
    let mut filled_count = 0;
    
    for y in 0..grid_height {
        // High Y index = South (further from origin 50N)
        let current_lat_idx = max_lat_idx - y as i32;
        let lat = get_lat(current_lat_idx);

        for x in 0..grid_width {
            let current_lon_idx = min_lon_idx + x as i32;
            let lon = get_lon(current_lon_idx);
            
            let mut resolved_type = None;

            // A) Priority 1: Manual Geographic Tile Map (exact GPS mapping)
            if let Some(tile_type) = geo_tile_map.get(&(current_lat_idx, current_lon_idx)) {
                resolved_type = Some(tile_type.clone());
            }

            // B) Priority 2: Game Sheets (Interplay rubbersheeting)
            if resolved_type.is_none() {
                for (game_idx, sheet_opt) in game_sheets.iter().enumerate() {
                    if let Some((sheet, terrain_scale)) = sheet_opt {
                        if let Some((gx, gy)) = sheet.inverse_transform(lat, lon) {
                            // Boundary Check: Ensure we don't extrapolate far outside the 28x30 tile grid
                            // which could lead to artifacts bleeding between maps.
                            if gx < -20.0 || gx > (28.0 * terrain_scale + 20.0) || 
                               gy < -20.0 || gy > (30.0 * terrain_scale + 20.0) {
                                continue;
                            }

                            let rx = (gx / (*terrain_scale)).round() as i32;
                            let ry = (gy / (*terrain_scale)).round() as i32;
                            
                            if let Some(tile_type) = game_tile_maps[game_idx].get(&(rx, ry)) {
                                resolved_type = Some(tile_type.clone());
                                break; // First hit wins
                            }
                        }
                    }
                }
            }
            
            if let Some(t) = resolved_type {
                fixed_tiles.push(FixedTile { x: x as i32, y: y as i32, tile_type: t });
                filled_count += 1;
            } else {
                 fixed_tiles.push(FixedTile { x: x as i32, y: y as i32, tile_type: "unknown".to_string() });
            }
        }
    }
    
    println!("Filled {} tiles.", filled_count);

    // 5. Place Locations FIRST (on untrimmed grid)
    let mut all_locations_placed: Vec<FixedLocation> = Vec::new();
    
    for (name, lat, lon, icon_type) in all_real_world_locs.iter() {
        let f_lat_idx = (lat - universe_origin_lat) / LAT_STEP;
        let f_lon_idx = (lon - universe_origin_lon) / LON_STEP;
        let grid_x = f_lon_idx - (min_lon_idx as f32);
        let grid_y = (max_lat_idx as f32) - f_lat_idx;
        
        // Check for actual collision (markers overlapping, not just same tile)
        for existing_loc in &all_locations_placed {
            let dx = grid_x - existing_loc.x;
            let dy = grid_y - existing_loc.y;
            let distance = (dx * dx + dy * dy).sqrt();
            
            // Consider it a collision if markers are within 0.5 units
            if distance < 0.5 {
                println!("WARNING: Marker collision: '{}' overlaps '{}' (distance: {:.2})", 
                         name, existing_loc.name, distance);
                break;
            }
        }
        
        all_locations_placed.push(FixedLocation {
            name: name.clone(),
            x: grid_x,
            y: grid_y,
            icon_type: *icon_type,
        });
    }

    println!("DEBUG: Placed {} markers. Terrain has {} tiles.", all_locations_placed.len(), fixed_tiles.len());

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
    
    println!("Trimming from {}x{} to {}x{} (offset: {}, {})", 
             grid_width, grid_height, trimmed_width, trimmed_height, min_x_real, min_y_real);
    
    // Adjust tile coordinates
    let trimmed_tiles: Vec<FixedTile> = fixed_tiles.into_iter()
        .filter(|t| t.x >= min_x_real && t.x <= max_x_real && t.y >= min_y_real && t.y <= max_y_real)
        .map(|t| FixedTile {
            x: t.x - min_x_real,
            y: t.y - min_y_real,
            tile_type: t.tile_type,
        })
        .collect();

    // Adjust marker coordinates
    let trimmed_locations: Vec<FixedLocation> = all_locations_placed.into_iter()
        .map(|loc| FixedLocation {
            name: loc.name,
            x: loc.x - min_x_real as f32,
            y: loc.y - min_y_real as f32,
            icon_type: loc.icon_type,
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
    
    println!("Built RubberSheet with {} control points", control_points.len());
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
    let hex_height_deg = MILES_PER_HEX / MILES_PER_DEGREE_LAT; // ~0.087
    let hex_width_deg = MILES_PER_HEX / MILES_PER_DEGREE_LON_AT_35N; // ~0.106
    
    let grid_height = (lat_range / hex_height_deg).ceil() as u32;
    let grid_width = (lon_range / hex_width_deg).ceil() as u32;
    
    println!("Inverse Warping Grid: {}x{} hexes (spanning {:.2} deg lat, {:.2} deg lon)", 
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
            });
        }
    }
    
    println!("Generated {} Sand tiles for marker-only map", fixed_tiles.len());
    
    // 5. Place Locations
    let final_locations: Vec<FixedLocation> = real_world_locations.iter().map(|(name, lat, lon, icon_type)| {
        let u = (lon - min_lon) / lon_range;
        let v = (max_lat - lat) / lat_range; // Inverted Y logic matches loop
        
        FixedLocation {
            name: name.clone(),
            x: u * grid_width as f32,
            y: v * grid_height as f32,
            icon_type: *icon_type,
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
