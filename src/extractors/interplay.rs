use crate::schema::{RawMapData, RawLocation, RawTile, IconType};
use crate::extractors::Extractor;
use crate::dat_extractor;
use std::io;
use std::path::{Path, PathBuf};
use std::fs;

#[derive(Debug, Clone, Copy)]
pub enum GameVariant {
    Fallout1,
    Fallout2,
}

impl GameVariant {
    fn app_id(&self) -> u32 {
        match self {
            GameVariant::Fallout1 => 38400,
            GameVariant::Fallout2 => 38410,
        }
    }

    fn name(&self) -> &str {
        match self {
            GameVariant::Fallout1 => "Fallout 1",
            GameVariant::Fallout2 => "Fallout 2",
        }
    }
    
    fn id_str(&self) -> &str {
        match self {
            GameVariant::Fallout1 => "fallout1",
            GameVariant::Fallout2 => "fallout2",
        }
    }
}

pub struct FalloutInterplayExtractor {
    variant: GameVariant,
}

impl FalloutInterplayExtractor {
    pub fn new(variant: GameVariant) -> Self {
        Self { variant }
    }
}

impl Extractor for FalloutInterplayExtractor {
    fn get_id(&self) -> &str {
        self.variant.id_str()
    }

    fn extract(&self) -> RawMapData {
        match self.extract_real_data() {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Error extracting {} data: {}", self.variant.name(), e);
                eprintln!("Falling back to mock data");
                self.extract_mock_data()
            }
        }
    }
}

impl FalloutInterplayExtractor {
    fn extract_real_data(&self) -> io::Result<RawMapData> {
        println!("Extracting {} Data...", self.variant.name());
        
        // 1. Find Game Install (Fallout 2 Only)
        let install_dir = if let GameVariant::Fallout2 = self.variant {
            dat_extractor::find_steam_app(self.variant.app_id(), self.variant.name()).ok()
        } else {
            None
        };
        
        // 2. Load CITY.TXT
        let city_txt = self.load_text_file(&install_dir, "CITY.TXT", "city.txt")?;
        
        // 3. Load WORLDMAP.TXT
        let worldmap_txt = self.load_text_file(&install_dir, "WORLDMAP.TXT", "worldmap.txt")?;
        
        // 4. Parse Data
        let locations = self.parse_city_txt(&city_txt)?;
        let (tiles, width, height) = self.parse_worldmap_txt(&worldmap_txt)?;
        
        println!("Extracted {}x{} terrain grid with {} tiles", width, height, tiles.len());
        
        Ok(RawMapData {
            locations,
            tiles,
            width: width as u32,
            height: height as u32,
            coordinate_scale: 50.0,
            reference_lat: None,
            reference_lon: None,
            terrain_data: Some(worldmap_txt),
        })
    }
    
    fn load_text_file(&self, install_dir: &Option<PathBuf>, filename: &str, cache_name: &str) -> io::Result<String> {
        // Strategy:
        // 1. Check Cache
        // 2. Check Local File (Fallout 2 Only)
        // 3. Download from GitHub (Fallout 1 Only)
        
        let cache_path = Path::new("output/.cache").join(self.variant.id_str()).join(cache_name);
        if cache_path.exists() {
            println!("Using cached {}...", filename);
            return fs::read_to_string(&cache_path);
        }
        
        let mut content: Option<String> = None;
        
        // Try Local File (Fallout 2)
        if let GameVariant::Fallout2 = self.variant {
            if let Some(dir) = install_dir {
                // 1. Try Loose File
                let local_path = dir.join("data").join(filename);
                if local_path.exists() {
                     println!("Found local file: {:?}", local_path);
                     content = Some(fs::read_to_string(local_path)?);
                } else {
                     let local_path_lower = dir.join("data").join(filename.to_lowercase());
                     if local_path_lower.exists() {
                         println!("Found local file: {:?}", local_path_lower);
                         content = Some(fs::read_to_string(local_path_lower)?);
                     }
                }
                
                // 2. Try DAT Archive (master.dat)
                if content.is_none() {
                    let master_dat = dir.join("master.dat");
                    if master_dat.exists() {
                        let inner_path = format!("data/{}", filename); // e.g. "data/CITY.TXT"
                        println!("Looking for {} inside {:?}", inner_path, master_dat);
                        match dat_extractor::extract_dat2(&master_dat, &inner_path) {
                            Ok(bytes) => {
                                println!("Found {} in master.dat", inner_path);
                                // decode assuming latin1 or utf8? Fallout 1/2 used varied encoding but usually ASCII/Latin1.
                                // output is likely valid UTF-8 compatible if English.
                                // Use String::from_utf8_lossy
                                content = Some(String::from_utf8_lossy(&bytes).to_string());
                            },
                            Err(_) => {
                                // Try lowercase internal path?
                                let inner_path_lower = format!("data/{}", filename.to_lowercase());
                                println!("Looking for {} inside {:?}", inner_path_lower, master_dat);
                                match dat_extractor::extract_dat2(&master_dat, &inner_path_lower) {
                                    Ok(bytes) => {
                                        println!("Found {} in master.dat", inner_path_lower);
                                        content = Some(String::from_utf8_lossy(&bytes).to_string());
                                    },
                                    Err(e) => println!("Could not find {} in master.dat: {}", filename, e),
                                }
                            }
                        }
                    } else {
                        println!("master.dat not found at {:?}", master_dat);
                    }
                }
            }
        }
        
        // Try GitHub (Fallout 1)
        if content.is_none() {
            if let GameVariant::Fallout1 = self.variant {
                 println!("Downloading {} from GitHub...", filename);
                 let url = if filename.to_uppercase() == "CITY.TXT" {
                     "https://raw.githubusercontent.com/rotators/Fo1in2/refs/heads/master/Fallout2/Fallout1in2/mods/fo1_base/data/CITY.TXT"
                 } else {
                     "https://raw.githubusercontent.com/rotators/Fo1in2/refs/heads/master/Fallout2/Fallout1in2/mods/fo1_base/data/WORLDMAP.TXT"
                 };
                 
                 let response = ureq::get(url).call().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
                 content = Some(response.into_string()?);
            }
        }

        if let Some(data) = content {
            // Cache it
            if let Some(parent) = cache_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&cache_path, &data)?;
            println!("Cached {} for future use", filename);
            Ok(data)
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, format!("Could not find {}", filename)))
        }
    }

    fn remap_location_name(&self, name: &str) -> String {
        // Shared remapping logic
        match name {
            "Military Base" => "Mariposa Military Base".to_string(),
            "Glow" => "The Glow".to_string(),
            "Hub" => "The Hub".to_string(),
            "Boneyard" => "The Boneyard".to_string(),
            "Cathedral" => "The Cathedral".to_string(),
            "Brotherhood of Steel" => "Lost Hill Bunker".to_string(),
            "NCR" => "Shady Sands".to_string(), // NCR is what Shady Sands is called in Fallout 2
            "Enclave" => "Enclave Oil Rig".to_string(),
            "abbey" => "Abbey".to_string(),
            "EPA" => "Environmental Protection Agency San Francisco Facility".to_string(),
            _ => name.to_string(),
        }
    }
    
    fn parse_city_txt(&self, content: &str) -> io::Result<Vec<RawLocation>> {
        let mut locations = Vec::new();
        let mut current_name: Option<String> = None;
        let mut current_pos: Option<(f32, f32)> = None;
        
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') { continue; }
            
            // Strip inline comments
            let clean_line = if let Some(idx) = line.find(';') {
                line[..idx].trim()
            } else {
                line
            };
            
            if clean_line.starts_with('[') && clean_line.ends_with(']') {
                if let (Some(name), Some((x, y))) = (current_name.take(), current_pos.take()) {
                    self.add_location_if_valid(&mut locations, name, x, y);
                }
                current_name = None;
                current_pos = None;
                continue;
            }
            
            if let Some((key, value)) = clean_line.split_once('=') {
                match key.trim() {
                    "area_name" => {
                        let val = value.trim();
                         // Format: "XX_Name" or just "Name"
                        let name = if let Some((_, rest)) = val.split_once('_') {
                            rest.to_string()
                        } else {
                            val.to_string()
                        };
                        current_name = Some(name);
                    }
                    "world_pos" => {
                        if let Some((x_str, y_str)) = value.trim().split_once(',') {
                            if let (Ok(x), Ok(y)) = (x_str.trim().parse::<f32>(), y_str.trim().parse::<f32>()) {
                                current_pos = Some((x, y));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        
        if let (Some(name), Some((x, y))) = (current_name, current_pos) {
            self.add_location_if_valid(&mut locations, name, x, y);
        }
        
        println!("Extracted {} locations from CITY.TXT", locations.len());
        Ok(locations)
    }

    fn add_location_if_valid(&self, locations: &mut Vec<RawLocation>, name: String, x: f32, y: f32) {
        let name_lower = name.to_lowercase();
        // Shared filter list
        let invalid = ["random", "shadow worlds", "cathedraldead", "vatsdead", "raiders", 
                      "shady sands radscorpion cave", "hub irwines farm", "car outta gas", 
                      "vipers", "scrapheap", "desert", "mountain", "ocean"];
        
        if !(invalid.iter().any(|&s| name_lower.contains(s)) || name_lower.starts_with("special")) && !name_lower.is_empty() {
             let final_name = self.remap_location_name(&name);
             let icon_type = IconType::from_name(&final_name);
             locations.push(RawLocation { 
                 name: final_name, x, y, icon_type,
                 real_lat: None, real_lon: None,
             });
        }
    }
    
    fn parse_worldmap_txt(&self, content: &str) -> io::Result<(Vec<RawTile>, u32, u32)> {
        // WORLDMAP.TXT format: tiles arranged 4 wide (FO1) or potentially different for FO2?
        // Assuming same format for now as they are same engine.
        // Fallout 2 "Worldmap.txt" logic is practically identical.
        
        const TILE_WIDTH: u32 = 7;
        const TILE_HEIGHT: u32 = 6;
        const NUM_HORIZONTAL_TILES: u32 = 4;
        
        let mut terrain_map: std::collections::HashMap<(u32, u32), String> = std::collections::HashMap::new();
        let mut current_tile_idx: Option<u32> = None;
        
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') { continue; }
            
            if line.starts_with("[Tile ") && line.ends_with(']') {
                if let Some(num_str) = line.strip_prefix("[Tile ").and_then(|s| s.strip_suffix(']')) {
                    current_tile_idx = num_str.trim().parse::<u32>().ok();
                }
                continue;
            }
            
            if let Some(tile_idx) = current_tile_idx {
                if let Some((coord_str, rest)) = line.split_once('=') {
                     // x_y=Terrain...
                    if let Some((x_str, y_str)) = coord_str.split_once('_') {
                        if let (Ok(local_x), Ok(local_y)) = (x_str.parse::<u32>(), y_str.parse::<u32>()) {
                            if let Some(terrain) = rest.split(',').next() {
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
        
        let max_x = terrain_map.keys().map(|(x, _)| x).max().copied().unwrap_or(0);
        let max_y = terrain_map.keys().map(|(_, y)| y).max().copied().unwrap_or(0);
        let width = max_x + 1;
        let height = max_y + 1;
        
        let mut tiles = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let terrain = terrain_map.get(&(x, y)).map(|s| s.as_str()).unwrap_or("Desert");
                let tile_type = match terrain {
                    "Mountain" => "Mountain",
                    "Ocean" => "Water",
                    "City" => "City",
                    _ => "Sand", 
                };
                tiles.push(RawTile { 
                    x: x as i32, 
                    y: y as i32, 
                    tile_type: tile_type.to_string(),
                    real_lat: None,
                    real_lon: None,
                });
            }
        }
        
        Ok((tiles, width, height))
    }
    
    fn extract_mock_data(&self) -> RawMapData {
        // Simple mock
        println!("Using mock {} data", self.variant.name());
        RawMapData {
            tiles: vec![],
            locations: vec![RawLocation { 
                name: "Mock City".to_string(), x: 10.0, y: 10.0, icon_type: IconType::City,
                real_lat: None, real_lon: None,
            }],
            width: 20,
            height: 20,
            coordinate_scale: 50.0,
            reference_lat: None,
            reference_lon: None,
            terrain_data: None,
        }
    }
}
