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

    fn find_localized_strings(&self, install_dir: &Path) -> Option<(PathBuf, String)> {
        let data_dir = install_dir.join("Data");
        
        // 1. Check for explicit .STRINGS files (older games or loose files)
        let strings_dir = data_dir.join("Strings");
        if strings_dir.exists() {
            let prefix = match self.variant {
                 BethesdaGameVariant::Fallout4 => "fallout4_en",
                 BethesdaGameVariant::Fallout3 => "fallout3_en",
                 BethesdaGameVariant::FalloutNV => "falloutnv_en",
                 BethesdaGameVariant::Fallout76 => "fallout76_en",
            };
            return Some((strings_dir, format!("strings/{}", prefix)));
        }

        // 2. Check for .ba2 archives (FO4/FO76)
        let interface_ba2 = data_dir.join("Fallout4 - Interface.ba2");
        if interface_ba2.exists() {
             return Some((interface_ba2, "strings/fallout4_en".to_string()));
        }
        
        let startup_ba2 = data_dir.join("Fallout4 - Startup.ba2");
        if startup_ba2.exists() {
             return Some((startup_ba2, "strings/fallout4_en".to_string()));
        }

        None
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
        let mut stdout = child.stdout.expect("Failed to open stdout");
        let mut reader = BufReader::new(stdout);
        let mut line_buf = Vec::new();

        while reader.read_until(b'\n', &mut line_buf)? > 0 {
            let line = String::from_utf8_lossy(&line_buf);
            let line = line.trim();
            if !line.starts_with("LTEX") { 
                line_buf.clear();
                continue; 
            }
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
            line_buf.clear();
        }
        info!("Extracted {} ground textures (LTEX) from {}", ltex_map.len(), self.plugin_name);
        Ok(ltex_map)
    }

    fn run_esmdump_refrs(&self, install_dir: &Path) -> io::Result<(Vec<RawLocation>, Vec<(f32, f32)>, Vec<(f32, f32)>)> {
        let esm_path = install_dir.join("Data").join(&self.plugin_name);
        if !esm_path.exists() {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("ESM not found: {:?}", esm_path)));
        }

        let mut cmd = Command::new(self.get_esmdump_path());
        cmd.arg(&esm_path);

        if let Some((loc_path, prefix)) = self.find_localized_strings(install_dir) {
            info!("Using localization: {:?} with prefix {}", loc_path, prefix);
            cmd.arg(loc_path).arg(prefix);
        }

        let child = cmd
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
        
        let mut stdout = child.stdout.expect("Failed to open stdout");
        let mut reader = BufReader::new(stdout);
        let mut line_buf = Vec::new();

        while reader.read_until(b'\n', &mut line_buf)? > 0 {
            let line = String::from_utf8_lossy(&line_buf);
            let line = line.trim();
            if !line.starts_with("WRLD") && !line.starts_with("CELL") { 
                line_buf.clear();
                continue; 
            }
            
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
                line_buf.clear();
                continue;
            }

            // To map to `IconType`, we try to parse name or EDID
        // `FULL` is usually [0x000344c3] in esmdump without string dicts. We will use EDID for the name fallback.
        // Remove 'MapMarker' from the EDID to make it cleaner
        let marker_edid_clean = edid.replace("MapMarker", "").replace("HeadingRef", "").replace("Alt", "");
        
        if is_map_marker {
            // Priority: FULL (column 4) > cleaned EDID
            let full_str = columns.get(4).unwrap_or(&"").trim().to_string();
            let marker_name = if !full_str.is_empty() && !full_str.starts_with('[') {
                full_str
            } else {
                marker_edid_clean.clone()
            };

            if !marker_name.is_empty() {
                let icon_type = IconType::from_name(&marker_edid_clean);
                    
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
            line_buf.clear();
        }

        info!("Extracted {} map markers, {} trees, {} rock formations from {}", locations.len(), trees.len(), rocks.len(), self.plugin_name);
        Ok((locations, trees, rocks))
    }



    fn run_fo4land_and_findwater(&self, install_dir: &Path, ltex_map: &std::collections::HashMap<u32, String>, warnings: &mut Vec<String>) -> io::Result<Vec<RawTile>> {
        let esm_filename = if self.plugin_name.to_lowercase().ends_with(".esm") {
            self.plugin_name.clone()
        } else {
            format!("{}.esm", self.plugin_name)
        };
        let esm_path = install_dir.join("Data").join(esm_filename);
        
        let mut seen_unmapped_textures = std::collections::HashSet::new();
        let temp_dir = std::env::temp_dir();
        let terrain_dds = temp_dir.join(format!("{}_ltex.dds", self.plugin_name));
        let water_dds = temp_dir.join(format!("{}_water.dds", self.plugin_name));
        let _hmap_dds = temp_dir.join(format!("{}_hmap.dds", self.plugin_name));

        let bin_dir = env!("FO76UTILS_BIN_DIR");
        
        // 1. Run findwater
        // Usage: findwater INFILE.ESM OUTFILE.DDS [OPTIONS...] worldID
        let findwater_bin = Path::new(bin_dir).join("findwater");
        let world_id = if self.worldspace.to_lowercase() == "commonwealth" { "0x0000003C" } else { &self.worldspace };
        
        let status = Command::new(&findwater_bin)
            .arg(&esm_path)
            .arg(&water_dds)
            .arg(world_id)
            .status();

        match status {
            Ok(s) if s.success() => {
                info!("findwater successful for {}", self.worldspace);
            }
            _ => {
                warnings.push(format!("findwater failed for {}. Water tiles will be missing.", self.worldspace));
                warn!("findwater failed for {}. Continuing to fo4land.", self.worldspace);
            }
        }

        // 2. Run fo4land (or btddump for 76)
        // Usage: fo4land INFILE.ESM OUTFILE.DDS FMT [XMIN YMIN XMAX YMAX] [worldID]
        let land_bin = match self.variant {
            BethesdaGameVariant::Fallout76 => Path::new(bin_dir).join("btddump"),
            _ => Path::new(bin_dir).join("fo4land"),
        };

        let world_id = if self.worldspace.to_lowercase() == "commonwealth" { "0x0000003C" } else { &self.worldspace };

        let mut cmd = Command::new(&land_bin);
        cmd.arg(&esm_path).arg(&terrain_dds);
        
        if self.variant == BethesdaGameVariant::Fallout76 {
            cmd.arg("1"); // mip level for btddump
        } else {
            // Force a square 256x256 cell range
            cmd.arg("6")
               .arg("-128").arg("-128").arg("128").arg("128")
               .arg(world_id);
        }

        let status = cmd.status()?;

        if !status.success() {
            warnings.push(format!("Terrain extraction failed for {}", self.worldspace));
            return Err(io::Error::new(io::ErrorKind::Other, "Terrain extraction failed"));
        }

        // 3. Sample DDS files efficiently
        let mut tiles = Vec::new();
        
        // Both tools usually dump the full world space if bounds are omitted or default.
        // For FO4, the world is roughly -128 to 128 cells (256x256 cells total).
        let world_cells = 256;
        
        let mut terrain_file = std::fs::File::open(&terrain_dds)?;
        let mut water_file = std::fs::File::open(&water_dds).ok();
        
        let mut header = [0u8; 128];
        terrain_file.read_exact(&mut header)?;
        let t_width = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
        let t_height = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
        info!("Terrain DDS: {}x{}", t_width, t_height);

        let mut w_width = 0;
        let mut w_height = 0;
        if let Some(ref mut f) = water_file {
            let mut w_header = [0u8; 128];
            if f.read_exact(&mut w_header).is_ok() {
                w_width = u32::from_le_bytes(w_header[16..20].try_into().unwrap()) as usize;
                w_height = u32::from_le_bytes(w_header[12..16].try_into().unwrap()) as usize;
                info!("Water DDS: {}x{}", w_width, w_height);
            }
        }

        let t_pixels_per_cell = t_width / world_cells;
        let w_pixels_per_cell = if w_width > 0 { w_width / world_cells } else { 0 };

        if t_pixels_per_cell == 0 {
            warnings.push(format!("Terrain extraction produced invalid dimensions for world_cells={}", world_cells));
            // Maybe its a smaller world, let's try assuming width IS current world cells
            // return self.extract_mock_data(warnings);
        }

        use std::io::{Seek, SeekFrom, Read};
        
        // Sampling range: enough to cover most worldspaces
        for cy in -128..128 {
            for sy in 0..16 {
                // Read horizontal strips
                let mut t_row_buf = Vec::new();
                let ty = ((cy + 128) as usize * t_pixels_per_cell) + (sy * t_pixels_per_cell / 16);
                if ty < t_height && t_pixels_per_cell > 0 {
                    t_row_buf = vec![0u8; t_width * 4];
                    terrain_file.seek(SeekFrom::Start(128 + (ty * t_width * 4) as u64))?;
                    terrain_file.read_exact(&mut t_row_buf)?;
                }

                let mut w_row_buf = Vec::new();
                if let Some(ref mut f) = water_file {
                    if w_pixels_per_cell > 0 {
                        let wy = ((cy + 128) as usize * w_pixels_per_cell) + (sy * w_pixels_per_cell / 16);
                        if wy < w_height {
                            w_row_buf = vec![0u8; w_width * 4];
                            f.seek(SeekFrom::Start(128 + (wy * w_width * 4) as u64))?;
                            f.read_exact(&mut w_row_buf)?;
                        }
                    }
                }

                if t_row_buf.is_empty() && w_row_buf.is_empty() { continue; }

                for cx in -128..128 {
                    for sx in 0..16 {
                        // Sample from buffers
                        let mut tile_type = "Misc".to_string();
                        let mut has_data = false;

                        if !t_row_buf.is_empty() {
                            let tx = ((cx + 128) as usize * t_pixels_per_cell) + (sx * t_pixels_per_cell / 16);
                            if tx < t_width {
                                let pixel_idx = tx * 4;
                                let form_id = u32::from_le_bytes(t_row_buf[pixel_idx..pixel_idx+4].try_into().unwrap());
                                if form_id != 0 {
                                    has_data = true;
                                    if let Some(texture_name) = ltex_map.get(&form_id) {
                                        if seen_unmapped_textures.insert(texture_name.clone()) {
                                            warnings.push(format!("Unmapped ground texture parsed: {}", texture_name));
                                        }
                                    }
                                }
                            }
                        }

                        if !w_row_buf.is_empty() {
                            let wx = ((cx + 128) as usize * w_pixels_per_cell) + (sx * w_pixels_per_cell / 16);
                            if wx < w_width {
                                if w_row_buf[wx*4] > 0 {
                                    tile_type = "Water".to_string();
                                    has_data = true;
                                }
                            }
                        }

                        if has_data {
                            tiles.push(RawTile {
                                x: (cx * 16 + sx as i32),
                                y: (cy * 16 + sy as i32),
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
        let (locations, _trees, _rocks) = self.run_esmdump_refrs(&install_path).unwrap_or_else(|e| {
            warn!("Failed to extract map markers/density via esmdump: {}", e);
            (vec![], vec![], vec![])
        });

        // 2. Terrain and Water (fo4land/findwater)
        let mut warnings = vec![];
        let mut tiles = self.run_fo4land_and_findwater(&install_path, &ltex_map, &mut warnings).unwrap_or_else(|e| {
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
        data.warnings = warnings;
        data.coordinate_system = crate::schema::CoordinateSystem::Relative;
        data.coordinate_scale = 256.0; // 16 tiles per cell
        
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
            coordinate_scale: 256.0, // 16 tiles per cell
            reference_lon: None,
            warnings: vec![],
            terrain_data: None,
        }
    }
}
