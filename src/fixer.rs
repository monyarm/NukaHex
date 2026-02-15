use crate::schema::{RawMapData, FixedMapData, FixedTile, FixedLocation};
use crate::rubbersheeting::{RubberSheet, ControlPoint};
use std::collections::HashMap;

// User-provided real-world coordinates (NAME ONLY - game coords come from CITY.TXT)
const FALLOUT1_REAL_COORDS: &[(&str, f32, f32)] = &[
    // (name, real_lat, real_lon)
    ("Shady Sands", 37.3767525, -118.4820828),
    ("Mariposa Military Base", 36.624861300000006, -121.78344730000002), 
    ("Lost Hill Bunker", 35.6081873, -119.3829345),  
    ("Junktown", 35.65363980000001,-117.4329572),
    ("The Hub", 34.1391501, -117.2897652),  
    ("Necropolis", 34.913266,-117.015381),  
    ("The Boneyard", 34.2070709,-118.3117676),
    ("The Cathedral", 33.8088202,-118.32000730000001),
    ("The Glow", 33.1467502,-116.85058590000001),
    ("Vault 13", 37.3679428,-118.9736945),
    ("Vault 15", 37.3964654,-118.2568593),
];

const MILES_PER_HEX: f32 = 6.0;
const MILES_PER_DEGREE_LAT: f32 = 69.0;
const MILES_PER_DEGREE_LON_AT_35N: f32 = 56.5;

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
fn fix_with_terrain_inverse_warping(raw_maps: Vec<RawMapData>) -> FixedMapData {
    // 1. Collect Data
    let mut all_locations = Vec::new();
    let mut all_tiles = Vec::new();
    for map in &raw_maps {
        all_locations.extend(map.locations.clone());
        all_tiles.extend(map.tiles.clone());
    }
    
    // Index raw tiles for O(1) sampling by (x, y)
    let mut raw_tile_map: HashMap<(i32, i32), String> = HashMap::new();
    for tile in &all_tiles {
        raw_tile_map.insert((tile.x as i32, tile.y as i32), tile.tile_type.clone());
    }

    // Fallout 1 world map squares are ~50 pixels in game coordinates
    // We scale terrain grid (0-28) to match location space (0-1400)
    let terrain_scale = 50.0;
    
    println!("Scaling terrain grid by {} to match game coordinates", terrain_scale);

    // 2. Build Rubbersheet (Control Points)
    // Map Game Name -> Game Coords
    let mut game_coords: HashMap<String, (f32, f32)> = HashMap::new();
    for loc in &all_locations {
        game_coords.insert(loc.name.to_lowercase(), (loc.x, loc.y));
    }
    
    let mut control_points = Vec::new();
    let mut real_world_locations = Vec::new(); // For final placement
    
    for (name, lat, lon) in FALLOUT1_REAL_COORDS {
        if let Some((gx, gy)) = game_coords.get(&name.to_lowercase()) {
            control_points.push(ControlPoint {
                name: name.to_string(),
                game_x: *gx,
                game_y: *gy,
                real_lat: *lat,
                real_lon: *lon,
            });
             if let Some(loc_data) = all_locations.iter().find(|l| l.name.eq_ignore_ascii_case(name)) {
                real_world_locations.push((name.to_string(), *lat, *lon, loc_data.icon_type));
            }
        }
    }
    
    println!("Built RubberSheet with {} control points", control_points.len());
    let sheet = RubberSheet::new(control_points);

    // 3. Define OUTPUT GRID Bounds based on Projected Game Grid CORNERS to ensure full coverage
    // Find grid extents from tiles to define the full game rectangle
    let min_tx = all_tiles.iter().map(|t| t.x).min().unwrap_or(0) as f32;
    let max_tx = all_tiles.iter().map(|t| t.x).max().unwrap_or(28) as f32;
    let min_ty = all_tiles.iter().map(|t| t.y).min().unwrap_or(0) as f32;
    let max_ty = all_tiles.iter().map(|t| t.y).max().unwrap_or(30) as f32;
    
    let min_gx = min_tx * terrain_scale;
    let max_gx = max_tx * terrain_scale;
    let min_gy = min_ty * terrain_scale;
    let max_gy = max_ty * terrain_scale;
    
    println!("Projecting Source Grid Bounds: ({}, {}) to ({}, {})", min_gx, min_gy, max_gx, max_gy);
    
    let corners = [
        (min_gx, min_gy), (max_gx, min_gy),
        (min_gx, max_gy), (max_gx, max_gy)
    ];
    
    let mut projected_lats = Vec::new();
    let mut projected_lons = Vec::new();
    
    for (gx, gy) in corners {
        let (lat, lon) = sheet.transform(gx, gy);
        projected_lats.push(lat);
        projected_lons.push(lon);
    }
    
    // Also include known locations to ensure convex hull validity
    projected_lats.extend(real_world_locations.iter().map(|(_, lat, _, _)| *lat));
    projected_lons.extend(real_world_locations.iter().map(|(_, _, lon, _)| *lon));

    let buffer = 0.2; // Slightly larger buffer for extrapolation safety
    
    let min_lat = projected_lats.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
    let max_lat = projected_lats.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
    let min_lon = projected_lons.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
    let max_lon = projected_lons.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
    
    let lat_range = max_lat - min_lat;
    let lon_range = max_lon - min_lon;
    
    let hex_height_deg = MILES_PER_HEX / MILES_PER_DEGREE_LAT;
    let hex_width_deg = MILES_PER_HEX / MILES_PER_DEGREE_LON_AT_35N;
    
    let grid_height = (lat_range / hex_height_deg).ceil() as u32;
    let grid_width = (lon_range / hex_width_deg).ceil() as u32;
    
    println!("Inverse Warping Grid: {}x{} hexes (spanning {:.2} deg lat, {:.2} deg lon)", 
             grid_width, grid_height, lat_range, lon_range);

    // 4. Inverse Sampling Loop
    let mut fixed_tiles = Vec::new();
    let mut filled_count = 0;
    
    for y in 0..grid_height {
        for x in 0..grid_width {
            let norm_y = y as f32 / grid_height as f32;
            let norm_x = x as f32 / grid_width as f32;
            
            let lat = max_lat - (norm_y * lat_range);
            let lon = min_lon + (norm_x * lon_range);
            
                if let Some((game_x, game_y)) = sheet.inverse_transform(lat, lon) {
                    // Convert Game Coords -> Grid Coords
                    let rx = (game_x / terrain_scale).round() as i32;
                    let ry = (game_y / terrain_scale).round() as i32;
                    
                    // Strict Bounds Check: Do NOT clamp. Unknown tiles must remain Unknown.
                    let min_x = min_tx as i32;
                    let max_x = max_tx as i32;
                    let min_y = min_ty as i32;
                    let max_y = max_ty as i32;
                    if rx >= min_x && rx <= max_x && ry >= min_y && ry <= max_y {
                        if let Some(terrain) = raw_tile_map.get(&(rx, ry)) {
                            fixed_tiles.push(FixedTile { x: x, y: y, tile_type: terrain.clone() });
                            filled_count += 1;
                        } else {
                            // Valid grid coordinate but no tile data? 
                            // This shouldn't happen if raw_tile_map is complete, but if it does, use "unknown" or "sand" fallback.
                            // Choosing "unknown" to highlight data gaps.
                            fixed_tiles.push(FixedTile { x: x, y: y, tile_type: "unknown".to_string() });
                        }
                    } else {
                        // Point maps to outside the source game grid -> Explicity "unknown" (Purple)
                        fixed_tiles.push(FixedTile { x: x, y: y, tile_type: "unknown".to_string() });
                    }
                } else {
                     // Inverse transform failed (shouldn't happen with extrapolation logic, but safe fallback)
                     fixed_tiles.push(FixedTile { x: x, y: y, tile_type: "unknown".to_string() });
                } 
        }
    }
    
    println!("Inverse Warping Complete. Filled {} tiles.", filled_count);
    
    // 5. Place Locations
    let final_locations: Vec<FixedLocation> = real_world_locations.iter().map(|(name, lat, lon, icon_type)| {
        let u = (lon - min_lon) / lon_range;
        let v = (max_lat - lat) / lat_range; 
        
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

    for (name, lat, lon) in FALLOUT1_REAL_COORDS {
        if let Some((gx, gy)) = game_coords.get(&name.to_lowercase()) {
            control_points.push(ControlPoint {
                name: name.to_string(),
                game_x: *gx,
                game_y: *gy,
                real_lat: *lat,
                real_lon: *lon,
            });
            // Also store for looking up icon type later
             if let Some(loc_data) = all_locations.iter().find(|l| l.name.eq_ignore_ascii_case(name)) {
                real_world_locations.push((name.to_string(), *lat, *lon, loc_data.icon_type));
            }
        }
    }
    
    println!("Built RubberSheet with {} control points", control_points.len());
    let sheet = RubberSheet::new(control_points);

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
                x,
                y,
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
