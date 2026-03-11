use crate::schema::{RawMapData, RawLocation, RawTile, IconType};
use crate::extractors::Extractor;
use crate::dat_extractor;
use std::io;
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use log::{info, error, warn};

#[derive(Debug, Clone, Copy)]
pub enum GameVariant {
    Fallout1,
    Fallout2,
    FalloutBoS,
}

impl GameVariant {
    fn app_id(&self) -> u32 {
        match self {
            GameVariant::Fallout1 => 38400,
            GameVariant::Fallout2 => 38410,
            GameVariant::FalloutBoS => 0, // Not on Steam typically
        }
    }

    fn name(&self) -> &str {
        match self {
            GameVariant::Fallout1 => "Fallout 1",
            GameVariant::Fallout2 => "Fallout 2",
            GameVariant::FalloutBoS => "Fallout: Brotherhood of Steel",
        }
    }
    
    fn id_str(&self) -> &str {
        match self {
            GameVariant::Fallout1 => "fallout1",
            GameVariant::Fallout2 => "fallout2",
            GameVariant::FalloutBoS => "fallout_bos",
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

    fn get_code_path(&self) -> &str {
        "src/extractors/interplay.rs"
    }

    fn get_cluster_id(&self) -> String {
        "interplay".to_string()
    }

    fn extract(&self) -> RawMapData {
        match self.extract_real_data() {
            Ok(data) => data,
            Err(e) => {
                error!("Error extracting {} data: {}", self.variant.name(), e);
                warn!("Falling back to mock data");
                self.extract_mock_data()
            }
        }
    }
}

impl FalloutInterplayExtractor {
    fn extract_real_data(&self) -> io::Result<RawMapData> {
        info!("Extracting {} Data...", self.variant.name());
        
        // 0. Check Config Overrides first
        let config_paths = crate::utils::load_config_paths().unwrap_or_default();
        let mut install_dir = config_paths.get(self.variant.id_str()).map(PathBuf::from);

        if install_dir.is_none() {
            // 1. Try Steam Discovery (only for PC games)
            install_dir = match self.variant {
                GameVariant::Fallout1 | GameVariant::Fallout2 => {
                    dat_extractor::find_steam_app(self.variant.app_id(), self.variant.name()).ok()
                }
                _ => None,
            };
        }

        if let Some(ref path) = install_dir {
            info!("Using installation directory: {:?}", path);
        }
        
        // 1. Load Metadata
        let city_txt = self.load_text_file(&install_dir, "CITY.TXT")?;
        let worldmap_txt = self.load_text_file(&install_dir, "WORLDMAP.TXT")?;

        // 3. Parse Locations
        // We always parse locations to ensure we have the data, then filter later if needed.
        let locations = self.parse_city_txt(&city_txt)?;
        


        // 4. Parse Worldmap Tiles
        let (tiles, width, height, _tile_masks) = self.parse_worldmap_txt(&worldmap_txt)?;

        info!("Final Extracted: {} base tiles, {} locations", tiles.len(), locations.len());

        Ok(RawMapData {
            locations,
            tiles,
            width: width as u32,
            height: height as u32,
            coordinate_system: crate::schema::CoordinateSystem::Absolute,
            rows: None,
            has_grid: false,
            coordinate_scale: 50.0,
            reference_lat: None,
            reference_lon: None,
            terrain_data: Some(worldmap_txt),
        })
    }
    
    fn load_text_file(&self, install_dir: &Option<PathBuf>, filename: &str) -> io::Result<String> {
        let cache_path = Path::new(".cache").join(self.variant.id_str()).join(filename);
        if cache_path.exists() {
            return fs::read_to_string(&cache_path);
        }
        
        // Try Local override or discovered directory
        if let Some(dir) = install_dir {
            // 1. Direct local file (data/FILENAME)
            let local_path = dir.join("data").join(filename);
            if local_path.exists() {
                let s = fs::read_to_string(local_path)?;
                if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
                fs::write(&cache_path, &s)?;
                return Ok(s);
            }

            // 2. Direct local file (FILENAME) - common for manual overrides
            let local_path_direct = dir.join(filename);
            if local_path_direct.exists() {
                let s = fs::read_to_string(local_path_direct)?;
                if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
                fs::write(&cache_path, &s)?;
                return Ok(s);
            }
            
            // 3. DAT (Only for DAT-compatible games)
            if let GameVariant::Fallout2 = self.variant {
                let master_dat = dir.join("master.dat");
                if master_dat.exists() {
                    let inner_path = format!("data/{}", filename.replace('/', "\\"));
                    if let Ok(bytes) = dat_extractor::extract_dat2(&master_dat, &inner_path) {
                        let s = String::from_utf8_lossy(&bytes).to_string();
                        if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
                        fs::write(&cache_path, &s)?;
                        return Ok(s);
                    }
                }
            }
        }
        
        // Try Remote (Fallout 1 via github)
        if let GameVariant::Fallout1 = self.variant {
            let base_url = "https://raw.githubusercontent.com/rotators/Fo1in2/master/Fallout2/Fallout1in2/mods/fo1_base";
            let url = if filename.to_lowercase() == "tiles.lst" {
                format!("{}/art/Tiles/TILES.LST", base_url)
            } else {
                format!("{}/data/{}", base_url, filename.replace('\\', "/"))
            };
            
            info!("[Downloader] fetching {}...", url);
            let response = ureq::get(&url).call().map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Download failed for {}: {}", url, e)))?;
            let s = response.into_string()?;
            if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
            fs::write(&cache_path, &s)?;
            return Ok(s);
        }
        
        Err(io::Error::new(io::ErrorKind::NotFound, format!("Text file {} not found (Variant: {:?})", filename, self.variant)))
    }

    fn load_binary_file(&self, install_dir: &Option<PathBuf>, filename: &str, cache_name: &str) -> io::Result<Vec<u8>> {
        // Ensure cache name doesn't lead to directory traversal and supports subfolders
        let safe_cache_name = cache_name.replace('\\', "/");
        let cache_path = Path::new(".cache").join(self.variant.id_str()).join(&safe_cache_name);
        
        if cache_path.exists() {
            return fs::read(&cache_path);
        }
        
        // Try Local override or discovered directory
        if let Some(dir) = install_dir {
            // 1. Direct local file (data/FILENAME)
            let local_path = dir.join("data").join(filename);
            if local_path.exists() {
                let data = fs::read(local_path)?;
                if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
                fs::write(&cache_path, &data)?;
                return Ok(data);
            }

            // 2. Direct local file (FILENAME)
            let local_path_direct = dir.join(filename);
            if local_path_direct.exists() {
                let data = fs::read(local_path_direct)?;
                if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
                fs::write(&cache_path, &data)?;
                return Ok(data);
            }
            
            // 3. DAT (Only for DAT-compatible games)
            if let GameVariant::Fallout2 = self.variant {
                let master_dat = dir.join("master.dat");
                if master_dat.exists() {
                    let inner_path = format!("data/{}", filename.replace('/', "\\"));
                    if let Ok(data) = dat_extractor::extract_dat2(&master_dat, &inner_path) {
                        info!("[Extractor] Extracted {} from DAT", filename);
                        if let Some(parent) = cache_path.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::write(&cache_path, &data)?;
                        return Ok(data);
                    }
                }
            }
        }
        
        // Try Remote (Fallout 1)
        if let GameVariant::Fallout1 = self.variant {
            let base_url = "https://raw.githubusercontent.com/rotators/Fo1in2/master/Fallout2/Fallout1in2/mods/fo1_base";
            // Ensure filename uses forward slashes for URL
            let clean_filename = filename.replace('\\', "/");
            let url = if clean_filename.to_lowercase().starts_with("maps/") {
                format!("{}/{}", base_url, clean_filename)
            } else {
                format!("{}/data/{}", base_url, clean_filename)
            };
            
            info!("[Downloader] fetching {}...", url);
            let response = ureq::get(&url).call().map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Download failed for {}: {}", url, e)))?;
            
            let mut data = Vec::new();
            response.into_reader().read_to_end(&mut data)?;
            
            if let Some(parent) = cache_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&cache_path, &data)?;
            return Ok(data);
        }
        
        Err(io::Error::new(io::ErrorKind::NotFound, format!("Binary file {} not found (Variant: {:?})", filename, self.variant)))
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
            "Enclave" => "Poseidon Energy Oil Rig".to_string(),
            "abbey" => "Abbey".to_string(),
            "EPA" => "Environmental Protection Agency San Francisco Facility".to_string(),
            _ => name.to_string(),
        }
    }

    fn resolve_marker_type(name: &str) -> IconType {
        match name {
            // Fallout 1
            "Vault 13" | "Vault 15" => IconType::Vault,
            "Shady Sands" | "Junktown" | "The Hub" | "Necropolis" | "The Boneyard" => IconType::City,
            "The Glow" | "Mariposa Military Base" | "The Cathedral" => IconType::Monument, // Or Ruins/Landmark? Using Monument for now.
            // Fallout 2
            "Vault City" | "San Francisco" | "New Reno" | "Redding" | "Broken Hills" | "Klamath" | "Modoc" | "Navarro" | "Gecko" | "Arroyo" | "The Den" => IconType::City,
            "Poseidon Energy Oil Rig" => IconType::Monument,
            // Fallback
            _ => IconType::from_name(name),
        }
    }
    
    fn parse_city_txt(&self, content: &str) -> io::Result<Vec<RawLocation>> {
        let mut locations = Vec::new();
        let mut current_name: Option<String> = None;
        let mut current_pos: Option<(f32, f32)> = None;
        let mut _current_map: Option<String> = None;
        
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') { continue; }
            
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
        
        info!("Extracted {} locations from CITY.TXT", locations.len());
        Ok(locations)
    }

    fn add_location_if_valid(&self, locations: &mut Vec<RawLocation>, name: String, x: f32, y: f32) {
        let name_lower = name.to_lowercase();
        let invalid = ["random", "shadow worlds", "cathedraldead", "vatsdead", "raiders", 
                      "shady sands radscorpion cave", "hub irwines farm", "car outta gas", 
                      "vipers", "scrapheap", "desert", "mountain", "ocean"];
        
        if !(invalid.iter().any(|&s| name_lower.contains(s)) || name_lower.starts_with("special")) && !name_lower.is_empty() {
             let final_name = self.remap_location_name(&name);
             let icon_type = Self::resolve_marker_type(&final_name);
             
             locations.push(RawLocation { 
                 name: final_name, x, y, icon_type,
                 real_lat: None, real_lon: None,
                 terrain_override: None,
                 anchor: None,
             });
        }
    }
    
    fn parse_worldmap_txt(&self, content: &str) -> io::Result<(Vec<RawTile>, u32, u32, HashMap<u32, String>)> {
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
                    weight: 1.0,
                    anchor: None,
                });
            }
        }
        
        Ok((tiles, width, height, HashMap::new()))
    }
    
    fn extract_mock_data(&self) -> RawMapData {
        // Simple mock
        warn!("Using mock {} data", self.variant.name());
        RawMapData {
            tiles: vec![],
            locations: vec![RawLocation { 
                name: "Mock City".to_string(), x: 10.0, y: 10.0, icon_type: IconType::City,
                real_lat: None, real_lon: None,
                terrain_override: None,
                anchor: None,
            }],
            width: 20,
            height: 20,
            coordinate_system: crate::schema::CoordinateSystem::Absolute,
            rows: None,
            has_grid: false,
            coordinate_scale: 50.0,
            reference_lat: None,
            reference_lon: None,
            terrain_data: None,
        }
    }
}
