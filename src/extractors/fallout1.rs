use crate::schema::{RawMapData, RawLocation, RawTile};
use crate::extractors::Extractor;
use std::io;

pub struct Fallout1Extractor;

impl Extractor for Fallout1Extractor {
    fn get_id(&self) -> &str {
        "fallout1"
    }

    fn extract(&self) -> RawMapData {
        match self.extract_real_data() {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Error extracting Fallout 1 data: {}", e);
                eprintln!("Falling back to mock data");
                self.extract_mock_data()
            }
        }
    }
}

impl Fallout1Extractor {
    fn extract_real_data(&self) -> io::Result<RawMapData> {
        println!("Extracting Fallout 1 Data...");
        
        // Download and cache CITY.TXT from GitHub
        const CITY_TXT_URL: &str = "https://raw.githubusercontent.com/rotators/Fo1in2/refs/heads/master/Fallout2/Fallout1in2/mods/fo1_base/data/CITY.TXT";
        const WORLDMAP_TXT_URL: &str = "https://raw.githubusercontent.com/rotators/Fo1in2/refs/heads/master/Fallout2/Fallout1in2/mods/fo1_base/data/WORLDMAP.TXT";
        
        let city_cache_path = std::path::Path::new("output/.cache/city.txt");
        let worldmap_cache_path = std::path::Path::new("output/.cache/worldmap.txt");
        
        let city_txt = if city_cache_path.exists() {
            println!("Using cached CITY.TXT...");
            std::fs::read_to_string(city_cache_path)?
        } else {
            println!("Downloading CITY.TXT from GitHub...");
            let response = ureq::get(CITY_TXT_URL)
                .call()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to download CITY.TXT: {}", e)))?;
            
            let city_txt = response.into_string()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to read response: {}", e)))?;
            
            // Cache it
            if let Some(parent) = city_cache_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(city_cache_path, &city_txt)?;
            println!("Cached CITY.TXT for future use");
            
            city_txt
        };
        
        let worldmap_txt = if worldmap_cache_path.exists() {
            println!("Using cached WORLDMAP.TXT...");
            std::fs::read_to_string(worldmap_cache_path)?
        } else {
            println!("Downloading WORLDMAP.TXT from GitHub...");
            let response = ureq::get(WORLDMAP_TXT_URL)
                .call()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to download WORLDMAP.TXT: {}", e)))?;
            
            let worldmap_txt = response.into_string()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to read response: {}", e)))?;
            
            // Cache it
            if let Some(parent) = worldmap_cache_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(worldmap_cache_path, &worldmap_txt)?;
            println!("Cached WORLDMAP.TXT for future use");
            
            worldmap_txt
        };
        
        // Parse locations from CITY.TXT
        let locations = self.parse_city_txt(&city_txt)?;
        
        // Parse terrain from WORLDMAP.TXT and generate tiles
        let (tiles, width, height) = self.parse_worldmap_txt(&worldmap_txt)?;
        
        println!("Extracted {}x{} terrain grid with {} tiles", width, height, tiles.len());
        
        Ok(RawMapData {
            tiles,
            locations,
            width,
            height,
            terrain_data: None, // Already parsed into tiles
        })
    }
    
    fn remap_location_name(&self, name: &str) -> String {
        // Map generic names to specific names
        match name {
            "Military Base" => "Mariposa Military Base".to_string(),
            "Glow" => "The Glow".to_string(),
            "Hub" => "The Hub".to_string(),
            "Boneyard" => "The Boneyard".to_string(),
            "Cathedral" => "The Cathedral".to_string(),
            "Brotherhood of Steel" => "Lost Hill Bunker".to_string(),
            // Add more mappings as needed
            _ => name.to_string(),
        }
    }
    
    fn parse_city_txt(&self, content: &str) -> io::Result<Vec<RawLocation>> {
        let mut locations = Vec::new();
        
        // CITY.TXT is in INI format:
        // [Area XX]
        // area_name=XX_Location Name
        // world_pos=X,Y
        // ...
        
        let mut current_name: Option<String> = None;
        let mut current_pos: Option<(f32, f32)> = None;
        
        for line in content.lines() {
            let line = line.trim();
            
            // Skip comments and empty lines
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            
            // Check for section start [Area XX]
            if line.starts_with('[') && line.ends_with(']') {
                // Save previous location if we have both name and position
                if let (Some(name), Some((x, y))) = (current_name.take(), current_pos.take()) {
                    // Filter out unwanted entries
                    let name_lower = name.to_lowercase();
                    if !name_lower.contains("random") && 
                       !["shadow worlds", "cathedraldead", "raiders", 
                       "shady sands radscorpion cave", "hub irwines farm", "car outta gas", "vipers", "scrapheap"].iter().any(|&s| s == &name_lower) &&
                       !name_lower.is_empty() {
                        // Remap generic names to specific names
                        let final_name = self.remap_location_name(&name);
                        let icon_type = crate::schema::IconType::from_name(&final_name);
                        locations.push(RawLocation {
                            name: final_name,
                            x,
                            y,
                            icon_type,
                        });
                    }
                }
                // Reset for new area
                current_name = None;
                current_pos = None;
                continue;
            }
            
            // Parse key=value pairs
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                
                match key {
                    "area_name" => {
                        // Format is typically "XX_Name" where XX is number prefix
                        let name = if let Some((_, rest)) = value.split_once('_') {
                            rest.to_string()
                        } else {
                            value.to_string()
                        };
                        current_name = Some(name);
                    }
                    "world_pos" => {
                        // Format is "X,Y"
                        if let Some((x_str, y_str)) = value.split_once(',') {
                            if let (Ok(x), Ok(y)) = (x_str.trim().parse::<f32>(), y_str.trim().parse::<f32>()) {
                                current_pos = Some((x, y));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        
        // Don't forget the last location
        if let (Some(name), Some((x, y))) = (current_name, current_pos) {
            let name_lower = name.to_lowercase();
            if !name_lower.contains("random") && 
               !name_lower.contains("shadow world") &&
               !name_lower.is_empty() {
                let icon_type = crate::schema::IconType::from_name(&name);
                locations.push(RawLocation {
                    name,
                    x,
                    y,
                    icon_type,
                });
            }
        }
        
        println!("Extracted {} locations from CITY.TXT", locations.len());
        Ok(locations)
    }
    
    fn parse_worldmap_txt(&self, content: &str) -> io::Result<(Vec<RawTile>, u32, u32)> {
        // WORLDMAP.TXT format: tiles arranged 4 wide
        // Each tile is 7x6 squares
        const TILE_WIDTH: u32 = 7;
        const TILE_HEIGHT: u32 = 6;
        const NUM_HORIZONTAL_TILES: u32 = 4;
        
        let mut terrain_map: std::collections::HashMap<(u32, u32), String> = std::collections::HashMap::new();
        let mut current_tile_idx: Option<u32> = None;
        
        for line in content.lines() {
            let line = line.trim();
            
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            
            // Track current tile
            if line.starts_with("[Tile ") && line.ends_with(']') {
                if let Some(num_str) = line.strip_prefix("[Tile ").and_then(|s| s.strip_suffix(']')) {
                    current_tile_idx = num_str.trim().parse::<u32>().ok();
                }
                continue;
            }
            
            // Parse terrain within current tile: x_y=TerrainType,rest...
            if let Some(tile_idx) = current_tile_idx {
                if let Some((coord_str, rest)) = line.split_once('=') {
                    if let Some((x_str, y_str)) = coord_str.split_once('_') {
                        if let (Ok(local_x), Ok(local_y)) = (x_str.parse::<u32>(), y_str.parse::<u32>()) {
                            if let Some(terrain) = rest.split(',').next() {
                                // Calculate global coordinates
                                let tile_row = tile_idx / NUM_HORIZONTAL_TILES;
                                let tile_col = tile_idx % NUM_HORIZONTAL_TILES;
                                let global_x = tile_col * TILE_WIDTH + local_x;
                                let global_y = tile_row * TILE_HEIGHT + local_y;
                                
                                terrain_map.insert((global_x, global_y), terrain.to_string());
                            }
                        }
                    }
                }
            }
        }
        
        if terrain_map.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "No terrain data found in WORLDMAP.TXT"));
        }
        
        // Determine map dimensions
        let max_x = terrain_map.keys().map(|(x, _)| x).max().copied().unwrap_or(0);
        let max_y = terrain_map.keys().map(|(_, y)| y).max().copied().unwrap_or(0);
        let width = max_x + 1;
        let height = max_y + 1;
        
        // Generate tiles
        let mut tiles = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let terrain = terrain_map.get(&(x, y)).map(|s| s.as_str()).unwrap_or("Desert");
                
                // Map terrain types to tile types
                let tile_type = match terrain {
                    "Mountain" => "Mountain",
                    "Ocean" => "Water",
                    "City" => "City",
                    _ => "Sand", // Desert and unknown
                };
                
                tiles.push(RawTile {
                    x,
                    y,
                    tile_type: tile_type.to_string(),
                });
            }
        }
        
        println!("Parsed WORLDMAP.TXT: found {} terrain squares on {}x{} grid", terrain_map.len(), width, height);
        
        let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for tile in &tiles {
        *type_counts.entry(tile.tile_type.clone()).or_insert(0) += 1;
    }
    println!("Extracted Terrain Counts:");
    for (t_type, count) in &type_counts {
        println!("  {}: {}", t_type, count);
    }

    Ok((tiles, width, height))
    }
    
    fn extract_mock_data(&self) -> RawMapData {
        // Fallback mock data (used when download fails)
        println!("Using mock Fallout 1 data (download failed)");
        
        let width = 20;
        let height = 20;
        let mut tiles = Vec::new();

        for y in 0..height {
            for x in 0..width {
                tiles.push(RawTile {
                    x,
                    y,
                    tile_type: "Sand".to_string(),
                });
            }
        }

        let locations = vec![
            RawLocation { 
                name: "Vault 13".to_string(), 
                x: 5.0, 
                y: 5.0, 
                icon_type: crate::schema::IconType::Vault,
            },
            RawLocation { 
                name: "Shady Sands".to_string(), 
                x: 10.0, 
                y: 8.0, 
                icon_type: crate::schema::IconType::City,
            },
        ];

        RawMapData {
            tiles,
            locations,
            width,
            height,
            terrain_data: None,
        }
    }
}
