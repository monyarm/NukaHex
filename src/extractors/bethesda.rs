use crate::schema::{RawMapData, RawLocation, RawTile, IconType};
use crate::extractors::Extractor;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use log::{info, error, warn};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BethesdaGameVariant {
    Fallout3,
    FalloutNV,
    Fallout4,
    Fallout76,
}

impl BethesdaGameVariant {
    fn app_id(&self) -> u32 {
        match self {
            BethesdaGameVariant::Fallout3 => 22370,
            BethesdaGameVariant::FalloutNV => 22380,
            BethesdaGameVariant::Fallout4 => 377160,
            BethesdaGameVariant::Fallout76 => 1151340,
        }
    }

    fn name(&self) -> &str {
        match self {
            BethesdaGameVariant::Fallout3 => "Fallout 3",
            BethesdaGameVariant::FalloutNV => "Fallout: New Vegas",
            BethesdaGameVariant::Fallout4 => "Fallout 4",
            BethesdaGameVariant::Fallout76 => "Fallout 76",
        }
    }
    
    fn id_str(&self) -> &str {
        match self {
            BethesdaGameVariant::Fallout3 => "fallout3",
            BethesdaGameVariant::FalloutNV => "falloutnv",
            BethesdaGameVariant::Fallout4 => "fallout4",
            BethesdaGameVariant::Fallout76 => "fallout76",
        }
    }
}

pub struct BethesdaExtractor {
    variant: BethesdaGameVariant,
    plugin_name: String,
    worldspace: String,
}

impl BethesdaExtractor {
    pub fn new(variant: BethesdaGameVariant, plugin_name: &str, worldspace: &str) -> Self {
        Self {
            variant,
            plugin_name: plugin_name.to_string(),
            worldspace: worldspace.to_string(),
        }
    }

    fn deduce_install_dir(&self) -> Option<PathBuf> {
        // 1. Check Config Overrides first (if supported)
        let config_paths = crate::utils::load_config_paths().unwrap_or_default();
        if let Some(p) = config_paths.get(self.variant.id_str()) {
            return Some(PathBuf::from(p));
        }

        // 2. Try Steam Discovery
        crate::dat_extractor::find_steam_app(self.variant.app_id(), self.variant.name()).ok()
    }
}

impl Extractor for BethesdaExtractor {
    fn get_id(&self) -> &str {
        self.variant.id_str()
    }

    fn get_code_path(&self) -> &str {
        "src/extractors/bethesda.rs"
    }

    fn get_cluster_id(&self) -> String {
        format!("beth_{}", self.worldspace.to_lowercase())
    }

    fn extract(&self) -> RawMapData {
        match self.extract_real_data() {
            Ok(data) => data,
            Err(e) => {
                error!("Error extracting {} data: {}", self.variant.name(), e);
                self.extract_mock_data()
            }
        }
    }
}

impl BethesdaExtractor {
    fn get_esmdump_path(&self) -> PathBuf {
        let bin_dir = env!("FO76UTILS_BIN_DIR");
        Path::new(bin_dir).join("esmdump")
    }

    fn run_esmdump_ltex(&self, install_dir: &Path) -> io::Result<std::collections::HashMap<u32, String>> {
        let esm_path = install_dir.join("Data").join(&self.plugin_name);
        if !esm_path.exists() {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("ESM not found: {:?}", esm_path)));
        }

        let child = Command::new(self.get_esmdump_path())
            .arg(&esm_path)
            .arg("-t")
            .arg("-i")
            .arg("LTEX")
            .arg("-edid")
            .stdout(Stdio::piped())
            .spawn()?;

        let mut ltex_map = std::collections::HashMap::new();
        let stdout = child.stdout.expect("Failed to open stdout");
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            let line = line?;
            if !line.starts_with("LTEX") { continue; }
            let columns: Vec<&str> = line.split('\t').collect();
            // TSV Format: Group, Record, FormID, EDID
            if columns.len() >= 4 {
                let form_id_str = columns[2].trim().trim_start_matches("0x");
                if let Ok(form_id) = u32::from_str_radix(form_id_str, 16) {
                    let edid = columns[3].trim().to_string();
                    if !edid.is_empty() {
                        ltex_map.insert(form_id, edid);
                    }
                }
            }
        }
        info!("Extracted {} ground textures (LTEX) from {}", ltex_map.len(), self.plugin_name);
        Ok(ltex_map)
    }

    fn run_esmdump_refrs(&self, install_dir: &Path) -> io::Result<(Vec<RawLocation>, Vec<(f32, f32)>, Vec<(f32, f32)>)> {
        let esm_path = install_dir.join("Data").join(&self.plugin_name);
        if !esm_path.exists() {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("ESM not found: {:?}", esm_path)));
        }

        let child = Command::new(self.get_esmdump_path())
            .arg(&esm_path)
            .arg("-t")
            .arg("-v")
            .arg("-i")
            .arg("REFR")
            .arg("-edid")
            .stdout(Stdio::piped())
            .spawn()?;

        let mut locations = Vec::new();
        let mut trees = Vec::new();
        let mut rocks = Vec::new();
        
        let stdout = child.stdout.expect("Failed to open stdout");
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            let line = line?;
            if !line.starts_with("WRLD") && !line.starts_with("CELL") { continue; }
            
            let columns: Vec<&str> = line.split('\t').collect();
            // Columns with -v mode: 
            // 0:Group 1:REFR 2:FormID 3:EDID 4:FULL 5:DESC 6:RNAM 
            // 7:NAME(BaseObj) 8:XTEL 9:Flags 10:Parent 11:X 12:Y 13:Z ...
            if columns.len() < 13 || columns[1] != "REFR" { continue; }

            let edid = columns[3];
            let base_obj = columns[7].to_lowercase();
            let edid_lower = edid.to_lowercase();
            
            // Map markers are base object 0x00000010
            let is_map_marker = base_obj.contains("mapmarker") || base_obj.contains("0x00000010");
            let is_tree = base_obj.contains("tree") || edid_lower.contains("tree");
            let is_rock = base_obj.contains("rock") || base_obj.contains("cliff") || base_obj.contains("mountain") 
                || edid_lower.contains("rock") || edid_lower.contains("cliff") || edid_lower.contains("mountain");

            let mut has_coords = false;
            let mut x = 0.0;
            let mut y = 0.0;

            if let (Ok(px), Ok(py)) = (columns[11].trim().parse::<f32>(), columns[12].trim().parse::<f32>()) {
                x = px;
                y = py;
                has_coords = true;
            }

            if !has_coords {
                if is_map_marker && locations.len() < 2 {
                    info!("MapMarker dropped due to no coordinates.");
                }
                continue;
            }

            // To map to `IconType`, we try to parse name or EDID
            // `FULL` is usually [0x000344c3] in esmdump without string dicts. We will use EDID for the name fallback.
            // Remove 'MapMarker' from the EDID to make it cleaner
            let mut marker_name = edid.replace("MapMarker", "").replace("HeadingRef", "").replace("Alt", "");
            
            if is_map_marker && !marker_name.is_empty() {
                // User requested to completely exclude hidden/unmarked markers.
                // Unnamed markers have no FULL string, meaning they are hidden/internal quest markers.
                let full_str = columns.get(4).unwrap_or(&"").trim();
                let has_name = !full_str.is_empty() && full_str != "[]";
                
                if has_name {
                    let icon_type = IconType::from_name(&marker_name);
                    
                    locations.push(RawLocation {
                        name: marker_name,
                        x,
                        y,
                        icon_type,
                        real_lat: None,
                        real_lon: None,
                        terrain_override: None,
                        anchor: None,
                    });
                }
            } else if is_tree {
                trees.push((x, y));
            } else if is_rock {
                rocks.push((x, y));
            }
        }

        info!("Extracted {} map markers, {} trees, {} rock formations from {}", locations.len(), trees.len(), rocks.len(), self.plugin_name);
        Ok((locations, trees, rocks))
    }



    fn run_fo4land_and_findwater(&self, install_dir: &Path, ltex_map: &std::collections::HashMap<u32, String>) -> io::Result<Vec<RawTile>> {
        let esm_path = install_dir.join("Data").join(format!("{}.esm", self.plugin_name));
        
        let mut seen_unmapped_textures = std::collections::HashSet::new();
        let temp_dir = std::env::temp_dir();
        let terrain_dds = temp_dir.join(format!("{}_ltex.dds", self.plugin_name));
        let water_dds = temp_dir.join(format!("{}_water.dds", self.plugin_name));
        let _hmap_dds = temp_dir.join(format!("{}_hmap.dds", self.plugin_name));

        let bin_dir = env!("FO76UTILS_BIN_DIR");
        
        // 1. Run findwater
        let findwater_bin = Path::new(bin_dir).join("findwater");
        let fw_status = Command::new(&findwater_bin)
            .arg(&esm_path)
            .arg(&water_dds)
            .arg(&_hmap_dds)
            .arg(install_dir)
            .status();
            
        if let Err(e) = fw_status {
            warn!("Failed to run findwater: {}", e);
        }

        // 2. Run fo4land (or btddump for 76)
        let is_76 = matches!(self.variant, BethesdaGameVariant::Fallout76);
        
        let terrain_status = if is_76 {
            let btddump_bin = Path::new(bin_dir).join("btddump");
            let btd_path = install_dir.join("Data").join("Terrain").join("Appalachia.btd"); // Example default
            Command::new(&btddump_bin)
                .arg(btd_path)
                .arg(&terrain_dds)
                .arg("9") // format 9 = 32-bit form IDs
                .args(["-100", "-100", "100", "100"]) // Default dimensions
                .status()
        } else {
            let fo4land_bin = Path::new(bin_dir).join("fo4land");
            Command::new(&fo4land_bin)
                .arg(&esm_path)
                .arg(&terrain_dds)
                .arg("6") // format 6 = 32-bit Form IDs
                .status()
        };

        if let Err(e) = terrain_status {
            warn!("Failed to run terrain extractor: {}", e);
        }

        let mut tiles = Vec::new();

        // 3. Parse the resulting DDS files
        if std::fs::metadata(&terrain_dds).is_ok() {
            let bytes = std::fs::read(&terrain_dds)?;
            if bytes.len() > 128 {
                let height = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
                let width = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
                
                let pixel_data = &bytes[128..];
                let expected_len = width * height * 4;
                if pixel_data.len() >= expected_len {
                    for y in 0..height {
                        for x_group in 0..(width / 16) {
                            let base_x = x_group * 16;
                            let pixel_idx = (y * width + base_x) * 4;
                            let form_id = u32::from_le_bytes(pixel_data[pixel_idx..pixel_idx+4].try_into().unwrap());
                            
                            let tile_type = match form_id {
                                0 => "Misc".to_string(), // Empty or null
                                _ => {
                                    if let Some(texture_name) = ltex_map.get(&form_id) {
                                        let tn_loc = texture_name.to_lowercase();
                                        if tn_loc.contains("grass") || tn_loc.contains("forest") || tn_loc.contains("leaves") || tn_loc.contains("moss") {
                                            "Grass".to_string()
                                        } else if tn_loc.contains("sand") || tn_loc.contains("beach") || tn_loc.contains("desert") || tn_loc.contains("mojave") || tn_loc.contains("dirt") {
                                            "Sand".to_string()
                                        } else if tn_loc.contains("rock") || tn_loc.contains("stone") || tn_loc.contains("mountain") {
                                            "Mountain".to_string()
                                        } else if tn_loc.contains("rubble") || tn_loc.contains("debris") {
                                            "Rock".to_string()
                                        } else if tn_loc.contains("water") || tn_loc.contains("river") || tn_loc.contains("ocean") || tn_loc.contains("coast") {
                                            "Water".to_string()
                                        } else if tn_loc.contains("city") || tn_loc.contains("street") || tn_loc.contains("asphalt") || tn_loc.contains("concrete") || tn_loc.contains("pavement") || tn_loc.contains("cobble") {
                                            "City".to_string()
                                        } else {
                                            if seen_unmapped_textures.insert(texture_name.clone()) {
                                                warn!("Unmapped ground texture parsed: {}", texture_name);
                                            }
                                            "Misc".to_string()
                                        }
                                    } else {
                                        if seen_unmapped_textures.insert(format!("FormID:{:#010X}", form_id)) {
                                            warn!("Unmapped unseen ground texture FormID: {:#010X}", form_id);
                                        }
                                        "Misc".to_string()
                                    }
                                }
                            };

                            tiles.push(RawTile {
                                x: x_group as i32,
                                y: y as i32,
                                tile_type,
                                real_lat: None,
                                real_lon: None,
                                weight: 1.0,
                                anchor: None,
                            });
                        }
                    }
                }
            }
        }
        
        // Cleanup temp files
        let _ = std::fs::remove_file(&terrain_dds);
        let _ = std::fs::remove_file(&water_dds);
        let _ = std::fs::remove_file(&_hmap_dds);

        Ok(tiles)
    }

    fn extract_real_data(&self) -> io::Result<RawMapData> {
        info!("Extracting {} Data for worldspace {}...", self.variant.name(), self.worldspace);

        let install_dir = self.deduce_install_dir();
        let install_path = if let Some(ref path) = install_dir {
            info!("Using installation directory: {:?}", path);
            path.clone()
        } else {
            let msg = format!("Installation not found for {}.", self.variant.name());
            warn!("{}", msg);
            return Err(io::Error::new(io::ErrorKind::NotFound, msg));
        };

        let ltex_map = self.run_esmdump_ltex(&install_path).unwrap_or_default();

        // 1. Map markers and density (esmdump)
        let (mut locations, _trees, _rocks) = self.run_esmdump_refrs(&install_path).unwrap_or_else(|e| {
            warn!("Failed to extract map markers/density via esmdump: {}", e);
            (vec![], vec![], vec![])
        });

        // 2. Terrain and Water (fo4land/findwater)
        let mut tiles = self.run_fo4land_and_findwater(&install_path, &ltex_map).unwrap_or_else(|e| {
            warn!("Failed to extract terrain/water: {}", e);
            vec![]
        });

        // 3. Apply Density Overlays (Trees / Rocks)
        // We need to map the raw (x, y) coordinates from esmdump (which are game units, like 146368.4, -141504.4)
        // to our generic RawTile grid. 
        // For Fallout 4, 1 cell = 4096 game units. For FO76, 1 cell = 4096. 
        // We assumed a 201x201 cell grid in fo4land/btddump, from -100 to 100.
        // And each cell was split into 4 quadrants (effectively 16x1 but logically representing 2x2 grid per cell?)
        // Let's use a simplified bounding box check or nearest neighbor mapping for the trees and rocks.
        let cell_size = 4096.0;
        let map_origin_x = -100.0 * cell_size;
        let map_origin_y = -100.0 * cell_size;
        let tiles_per_cell = 2; // based on our width parsing logic earlier (assuming 201 cells * 2)

        for tile in tiles.iter_mut() {
            // Reconstruct the game coordinates for this tile
            // A tile represents a portion of a cell.
            let tile_game_x = map_origin_x + (tile.x as f32 / tiles_per_cell as f32) * cell_size;
            let tile_game_y = map_origin_y + (tile.y as f32 / tiles_per_cell as f32) * cell_size;
            let tile_game_w = cell_size / tiles_per_cell as f32;

            let mut tree_count = 0;
            let mut rock_count = 0;

            for &(tx, ty) in &_trees {
                if tx >= tile_game_x && tx < tile_game_x + tile_game_w &&
                   ty >= tile_game_y && ty < tile_game_y + tile_game_w {
                       tree_count += 1;
                }
            }

            for &(rx, ry) in &_rocks {
                if rx >= tile_game_x && rx < tile_game_x + tile_game_w &&
                   ry >= tile_game_y && ry < tile_game_y + tile_game_w {
                       rock_count += 1;
                }
            }

            // Thresholds are arbitrary for now, to be tuned.
            if rock_count > 10 {
                tile.tile_type = "Mountain".to_string();
            } else if tree_count > 15 {
                tile.tile_type = "Forest".to_string();
            }
        }

        // 4. Formulate the cluster ID
        let mut data = self.extract_mock_data();
        data.locations = locations;
        data.tiles = tiles;
        data.coordinate_system = crate::schema::CoordinateSystem::Relative;
        data.coordinate_scale = cell_size; // Game units per tile
        
        Ok(data)
    }

    fn extract_mock_data(&self) -> RawMapData {
        RawMapData {
            tiles: vec![],
            locations: vec![],
            width: 0,
            height: 0,
            coordinate_system: crate::schema::CoordinateSystem::Absolute,
            rows: None,
            has_grid: false,
            // 4096 is the standard unit length for one cell in Fallout games
            coordinate_scale: 4096.0,
            reference_lat: None,
            reference_lon: None,
            terrain_data: None,
        }
    }
}
