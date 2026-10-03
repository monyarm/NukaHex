use crate::schema::{RawMapData, RawLocation, RawTile, IconType, MapBounds};
use crate::extractors::Extractor;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use log::{info, error, warn};

use std::fs::{self, File};
use reqwest::blocking::get;
use zip::read::ZipArchive;
use flate2::read::GzDecoder;
use sha2::Digest;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BethesdaGameVariant {
    Fallout3,
    FalloutNV,
    Fallout4,
    Fallout76,
}

impl BethesdaGameVariant {
    /// Source priority tier: FO3/NV/4 are tier 1; FO76 is tier 2 (early in the timeline).
    fn priority(&self) -> u8 {
        match self { BethesdaGameVariant::Fallout76 => 2, _ => 1 }
    }

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
    plugins: Vec<String>,
    worldspace: String,
}

impl BethesdaExtractor {
    pub fn new(variant: BethesdaGameVariant, plugins: Vec<String>, worldspace: &str) -> Self {
        Self {
            variant,
            plugins,
            worldspace: worldspace.to_string(),
        }
    }

    fn deduce_install_dir(&self) -> Option<PathBuf> {
        let config_paths = crate::utils::load_config_paths().unwrap_or_default();
        if let Some(p) = config_paths.get(self.variant.id_str()) {
            return Some(PathBuf::from(p));
        }
        crate::dat_extractor::find_steam_app(self.variant.app_id(), self.variant.name()).ok()
    }

    fn resolve_plugins(&self, install_dir: &Path) -> io::Result<Vec<PathBuf>> {
        let cache_dir = Path::new("input/cache/bethesda");
        fs::create_dir_all(cache_dir)?;
        let data_dir = install_dir.join("Data");

        let mut resolved = Vec::new();
        for plugin in &self.plugins {
            if plugin.starts_with("http") {
                let url_hash = sha2::Sha256::digest(plugin.as_bytes());
                let safe_name = hex::encode(url_hash);
                let target_path = cache_dir.join(&safe_name);
                
                if !target_path.exists() {
                    info!("Downloading plugin from URL: {}", plugin);
                    let mut response = get(plugin).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                    let mut file = File::create(&target_path)?;
                    io::copy(&mut response, &mut file)?;
                }

                if plugin.to_lowercase().ends_with(".zip") {
                    let file = File::open(&target_path)?;
                    let mut archive = ZipArchive::new(file)?;
                    for i in 0..archive.len() {
                        let mut file = archive.by_index(i)?;
                        let outpath = cache_dir.join(file.mangled_name());
                        if (*file.name()).ends_with('/') {
                            fs::create_dir_all(&outpath)?;
                        } else {
                            if let Some(p) = outpath.parent() {
                                fs::create_dir_all(p)?;
                            }
                            let mut outfile = File::create(&outpath)?;
                            io::copy(&mut file, &mut outfile)?;
                            if outpath.extension().map_or(false, |e| e == "esm" || e == "esp" || e == "esl") {
                                resolved.push(outpath);
                            }
                        }
                    }
                } else if plugin.to_lowercase().ends_with(".gz") {
                    let file = File::open(&target_path)?;
                    let mut decoder = GzDecoder::new(file);
                    let out_name = plugin.split('/').last().unwrap().trim_end_matches(".gz");
                    let outpath = cache_dir.join(out_name);
                    let mut outfile = File::create(&outpath)?;
                    io::copy(&mut decoder, &mut outfile)?;
                    resolved.push(outpath);
                } else {
                    resolved.push(target_path);
                }
            } else {
                let local_path = data_dir.join(plugin);
                if local_path.exists() {
                    resolved.push(local_path);
                } else {
                    resolved.push(PathBuf::from(plugin)); // Assume it might be an absolute or local relative path
                }
            }
        }
        Ok(resolved)
    }

    fn get_mesh_archives(&self, install_dir: &Path) -> Vec<PathBuf> {
        let mut archives = Vec::new();
        let data_dir = install_dir.join("Data");
        if let Ok(entries) = fs::read_dir(data_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "ba2" || ext == "bsa" {
                        let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
                        if name.contains("meshes") || (!name.contains("textures") && !name.contains("sounds") && !name.contains("voices") && !name.contains("interface")) {
                            archives.push(path);
                        }
                    }
                }
            }
        }
        archives
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
                RawMapData {
                    locations: vec![],
                    tiles: vec![],
                    width: 0,
                    height: 0,
                    coordinate_system: crate::schema::CoordinateSystem::Absolute,
                    rows: None,
                    has_grid: false,
                    coordinate_scale: 256.0,
                    priority: self.variant.priority(),
                    reference_lon: None,
                    warnings: vec![format!("Extraction failed: {}", e)],
                    terrain_data: None,
                    world_bounds: None,
                    ..Default::default()
                }
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

    fn run_esmdump_ltex(&self, plugin_paths: &[PathBuf]) -> io::Result<std::collections::HashMap<u32, String>> {
        let plugins_str = plugin_paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(",");
        
        let child = Command::new(self.get_esmdump_path())
            .arg(&plugins_str)
            .arg("-t")
            .arg("-i")
            .arg("LTEX")
            .arg("-edid")
            .stdout(Stdio::piped())
            .spawn()?;

        let mut ltex_map = std::collections::HashMap::new();
        let stdout = child.stdout.expect("Failed to open stdout");
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
        info!("Extracted {} ground textures (LTEX) from plugins", ltex_map.len());
        Ok(ltex_map)
    }

    fn run_esmdump_refrs(&self, install_dir: &Path, plugin_paths: &[PathBuf], world_id: &str) -> io::Result<(Vec<RawLocation>, Vec<(f32, f32)>, Vec<(f32, f32)>, Option<MapBounds>)> {
        let world_id_clean = world_id.trim_start_matches("0x").to_uppercase();
        let plugins_str = plugin_paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(",");
        
        let mut cmd = Command::new(self.get_esmdump_path());
        cmd.arg(&plugins_str);

        if let Some((loc_path, prefix)) = self.find_localized_strings(install_dir) {
            info!("Using localization: {:?} with prefix {}", loc_path, prefix);
            cmd.arg(loc_path).arg(prefix);
        }

        let child = cmd
            .arg("-v")
            .arg("-i")
            .arg("REFR")
            .arg("-i")
            .arg("CELL")
            .arg("-edid")
            .stdout(Stdio::piped())
            .spawn()?;

        let mut locations = Vec::new();
        let mut trees = Vec::new();
        let mut rocks = Vec::new();
        

        let stdout = child.stdout.expect("Failed to open stdout");
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::new();

        let mut current_cell_x = 0;
        let mut current_cell_y = 0;
        let mut current_edid = String::new();
        let mut current_full = String::new();
        let mut current_base_obj = String::new();
        let mut current_rnam = String::new();
        let mut in_refr = false;
        let mut is_global_refr = false;
        
        let mut min_cell_x = i32::MAX;
        let mut max_cell_x = i32::MIN;
        let mut min_cell_y = i32::MAX;
        let mut max_cell_y = i32::MIN;

        let world_id_upper = world_id.to_uppercase();
        let world_id_clean = world_id_upper.trim_start_matches("0X");

        while reader.read_until(b'\n', &mut buf)? > 0 {
            let line = String::from_utf8_lossy(&buf);
            let l = line.trim();
            
            if l.starts_with("WRLD:CELL:") || l.starts_with("CELL:CELL:") {
                in_refr = false;
                current_edid.clear();
                current_full.clear();
                current_base_obj.clear();
            } else if l.starts_with("CELL:REFR:") {
                in_refr = true;
                is_global_refr = false;
                current_edid.clear();
                current_full.clear();
                current_base_obj.clear();
            } else if l.starts_with("WRLD:REFR:") {
                in_refr = true;
                is_global_refr = true;
                current_edid.clear();
                current_full.clear();
                current_base_obj.clear();
            } else if l.starts_with(":XCLC") {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 3 {
                    current_cell_x = parts[1].parse::<i32>().unwrap_or(0);
                    current_cell_y = parts[2].parse::<i32>().unwrap_or(0);
                    
                    if current_cell_x < min_cell_x { min_cell_x = current_cell_x; }
                    if current_cell_x > max_cell_x { max_cell_x = current_cell_x; }
                    if current_cell_y < min_cell_y { min_cell_y = current_cell_y; }
                    if current_cell_y > max_cell_y { max_cell_y = current_cell_y; }
                }
            } else if l.starts_with(":EDID") {
                current_edid = l.replacen(":EDID", "", 1).trim().to_string();
            } else if l.starts_with(":FULL") {
                current_full = l.replacen(":FULL", "", 1).trim().to_string();
            } else if l.starts_with(":NAME") {
                current_base_obj = l.replacen(":NAME", "", 1).trim().to_string();
            } else if l.starts_with(":RNAM") {
                current_rnam = l.replacen(":RNAM", "", 1).trim().to_string();
            } else if l.starts_with(":DATA") && in_refr {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let (Ok(lx), Ok(ly)) = (parts[1].parse::<f32>(), parts[2].parse::<f32>()) {
                        let mut gx = lx;
                        let mut gy = ly;
                        
                        if !is_global_refr {
                            gx += current_cell_x as f32 * 4096.0;
                            gy += current_cell_y as f32 * 4096.0;
                        }

                        let world_match = if !current_rnam.is_empty() && !world_id_clean.is_empty() {
                            current_rnam.to_uppercase().contains(world_id_clean)
                        } else { true };

                        if world_match {
                            let base_lower = current_base_obj.to_lowercase();
                            let edid_lower = current_edid.to_lowercase();
                            
                            let is_map_marker = base_lower.contains("mapmarker") || base_lower.contains("0x00000010");
                            let is_tree = base_lower.contains("tree") || edid_lower.contains("tree");
                            let is_rock = base_lower.contains("rock") || base_lower.contains("cliff") || base_lower.contains("mountain") 
                                || edid_lower.contains("rock") || edid_lower.contains("cliff") || edid_lower.contains("mountain");

                            if is_map_marker {
                                let marker_edid_clean = current_edid.replace("MapMarker", "").replace("HeadingRef", "").replace("Alt", "");
                                let marker_name = if !current_full.is_empty() && !current_full.starts_with('[') {
                                    current_full.clone()
                                } else {
                                    marker_edid_clean.clone()
                                };

                                if !marker_name.is_empty() {
                                    locations.push(RawLocation {
                                        name: marker_name,
                                        x: gx,
                                        y: gy,
                                        icon_type: IconType::from_name(&marker_edid_clean),
                                        real_lat: None,
                                        real_lon: None,
                                        terrain_override: None,
                                        anchor: None,
                                    });
                                }
                            } else if is_tree {
                                trees.push((gx, gy));
                            } else if is_rock {
                                rocks.push((gx, gy));
                            }
                        }
                    }
                }
                in_refr = false;
            }
            buf.clear();
        }

        let bounds = if min_cell_x != i32::MAX {
            Some(MapBounds {
                min_x: min_cell_x,
                min_y: min_cell_y,
                max_x: max_cell_x,
                max_y: max_cell_y,
            })
        } else {
            None
        };

        info!("Extracted {} map markers, {} trees, {} rock formations from plugins", locations.len(), trees.len(), rocks.len());
        Ok((locations, trees, rocks, bounds))
    }



    fn run_fo4land_and_findwater(&self, install_dir: &Path, plugin_paths: &[PathBuf], ltex_map: &std::collections::HashMap<u32, String>, warnings: &mut Vec<String>) -> io::Result<(std::collections::HashMap<(i32, i32), RawTile>, u32, u32)> {
        let plugins_str = plugin_paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(",");
        let mesh_archives = self.get_mesh_archives(install_dir);

        let temp_dir = std::env::temp_dir();
        let terrain_dds = temp_dir.join(format!("{}_ltex.dds", self.worldspace));
        let water_dds = temp_dir.join(format!("{}_water.dds", self.worldspace));
        let hmap_dds = temp_dir.join(format!("{}_hmap.dds", self.worldspace));

        let bin_dir = env!("FO76UTILS_BIN_DIR");
        let land_bin = match self.variant {
            BethesdaGameVariant::Fallout76 => Path::new(bin_dir).join("btddump"),
            _ => Path::new(bin_dir).join("fo4land"),
        };
        let findwater_bin = Path::new(bin_dir).join("findwater");
        let world_id = if self.worldspace.to_lowercase() == "commonwealth" { "0x0000003C" } else { &self.worldspace };

        // 1. Run fo4land for heightmap (FMT 0)
        info!("Generating heightmap for {}...", self.worldspace);
        let mut hmap_cmd = Command::new(&land_bin);
        hmap_cmd.arg(&plugins_str).arg(&hmap_dds).arg("0").arg("-128").arg("-128").arg("128").arg("128").arg(world_id);
        let _ = hmap_cmd.status();

        // 2. Run findwater with heightmap and mesh archives
        info!("Running findwater for {}...", self.worldspace);
        let mut fw_cmd = Command::new(&findwater_bin);
        fw_cmd.arg(&plugins_str).arg(&water_dds);
        fw_cmd.arg(&hmap_dds);
        // findwater expects archives as separate arguments
        for archive in mesh_archives {
            fw_cmd.arg(archive);
        }
        fw_cmd.arg(world_id);
        
        let status = fw_cmd.status();

        match status {
            Ok(s) if s.success() => {
                info!("findwater successful for {}", self.worldspace);
            }
            _ => {
                warnings.push(format!("findwater failed for {}. Water tiles will be missing.", self.worldspace));
                warn!("findwater failed for {}. Continuing to fo4land.", self.worldspace);
            }
        }

        // 3. Run fo4land (or btddump for 76) with FMT 3
        info!("Extracting terrain textures for {}...", self.worldspace);
        let mut cmd = Command::new(&land_bin);
        cmd.arg(&plugins_str).arg(&terrain_dds);
        
        let mut texture_mapping = std::collections::HashMap::new();

        if self.variant == BethesdaGameVariant::Fallout76 {
            cmd.arg("3") // dithered 8-bit for btddump
               .arg("-128").arg("-128").arg("128").arg("128")
               .arg("0"); // level 0
        } else {
            cmd.arg("3")
               .arg("-128").arg("-128").arg("128").arg("128")
               .arg(world_id);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            warnings.push(format!("Terrain extraction failed for {}", self.worldspace));
            return Err(io::Error::new(io::ErrorKind::Other, "Terrain extraction failed"));
        }

        // Parse texture mapping from stdout
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        for line in stdout_str.lines() {
            if line.contains("Land texture") && line.contains("0x") {
                // Land texture 0: 0x00021336 ...
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let idx_part = parts[0].trim();
                    let id_part = parts[1].trim();
                    if let Some(idx_str) = idx_part.split_whitespace().last() {
                        if let Ok(idx) = idx_str.parse::<u8>() {
                            if let Some(id_str) = id_part.split_whitespace().next() {
                                if let Ok(form_id) = u32::from_str_radix(id_str.trim_start_matches("0x"), 16) {
                                    texture_mapping.insert(idx, form_id);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Sample DDS files efficiently
        let mut tiles = std::collections::HashMap::new();
        
        let mut terrain_file = std::fs::File::open(&terrain_dds)?;
        let mut water_file = std::fs::File::open(&water_dds).ok();
        let mut hmap_file = std::fs::File::open(&hmap_dds).ok();
        
        let mut header = [0u8; 128];
        terrain_file.read_exact(&mut header)?;
        let t_width = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
        let t_height = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
        
        let mut h_header = [0u8; 128];
        let mut cell_min_x = -128;
        let mut cell_max_x = 127;
        let mut cell_min_y = -128;
        let mut cell_max_y = 127;
        let mut pixels_per_cell = 32;

        if let Some(ref mut f) = hmap_file {
            if f.read_exact(&mut h_header).is_ok() {
                // Parse FO76Utils "LAND" header extras (stored in DDS reserved fields starting at offset 32)
                // findwater: hdrBuf[0]=0x444E414C ("LAND"), hdrBuf[2]=minX, hdrBuf[3]=minY, hdrBuf[5]=maxX, hdrBuf[6]=maxY, hdrBuf[9]=cellSize
                let magic = u32::from_le_bytes(h_header[32..36].try_into().unwrap());
                if magic == 0x444E414C {
                    cell_min_x = i32::from_le_bytes(h_header[40..44].try_into().unwrap());
                    cell_min_y = i32::from_le_bytes(h_header[44..48].try_into().unwrap());
                    cell_max_x = i32::from_le_bytes(h_header[52..56].try_into().unwrap());
                    cell_max_y = i32::from_le_bytes(h_header[56..60].try_into().unwrap());
                    pixels_per_cell = u32::from_le_bytes(h_header[68..72].try_into().unwrap()) as usize;
                }
            }
        }
        
        let world_cells_x = (cell_max_x - cell_min_x + 1) as usize;
        let world_cells_y = (cell_max_y - cell_min_y + 1) as usize;
        info!("World bounds: ({},{}) to ({},{}), pixels_per_cell={}", cell_min_x, cell_min_y, cell_max_x, cell_max_y, pixels_per_cell);

        let mut w_width = 0;
        let mut w_height = 0;
        if let Some(ref mut f) = water_file {
            let mut w_header = [0u8; 128];
            if f.read_exact(&mut w_header).is_ok() {
                w_width = u32::from_le_bytes(w_header[16..20].try_into().unwrap()) as usize;
                w_height = u32::from_le_bytes(w_header[12..16].try_into().unwrap()) as usize;
            }
        }

        use std::io::{Seek, SeekFrom, Read};

        let tiles_per_dimension = 8;
        let samples_per_tile_dim = pixels_per_cell / tiles_per_dimension; 
        
        for cy in cell_min_y..=cell_max_y {
            for sy in 0..tiles_per_dimension {
                // Read 1 row for this row of tiles (we sample mid-point for speed or first pixel)
                // Actually let's read the full row for majority voting as before
                let mut texture_rows = Vec::new();
                let mut water_rows = Vec::new();
                let mut hmap_rows = Vec::new();

                for dy in 0..samples_per_tile_dim {
                    let row_idx = ((cell_max_y - cy) as usize * pixels_per_cell) + (sy * samples_per_tile_dim + dy);
                    
                    if row_idx < t_height {
                        let mut t_buf = vec![0u8; t_width];
                        terrain_file.seek(SeekFrom::Start(128 + (row_idx * t_width) as u64))?;
                        terrain_file.read_exact(&mut t_buf)?;
                        texture_rows.push(t_buf);
                    }

                    if let Some(ref mut f) = water_file {
                        if row_idx < w_height {
                            let mut w_buf = vec![0u8; w_width * 2];
                            f.seek(SeekFrom::Start(128 + (row_idx * w_width * 2) as u64))?;
                            f.read_exact(&mut w_buf)?;
                            water_rows.push(w_buf);
                        }
                    }

                    if let Some(ref mut f) = hmap_file {
                        if row_idx < (t_height) { // Heightmap is same size usually
                            let mut h_buf = vec![0u8; t_width * 2]; 
                            f.seek(SeekFrom::Start(128 + (row_idx * t_width * 2) as u64))?;
                            f.read_exact(&mut h_buf)?;
                            hmap_rows.push(h_buf);
                        }
                    }
                }

                if texture_rows.is_empty() { continue; }

                for cx in cell_min_x..=cell_max_x {
                    for sx in 0..tiles_per_dimension {
                        let mut water_count = 0;
                        let mut texture_votes = std::collections::HashMap::new();
                        let mut total_pixels = 0;

                        for dy in 0..texture_rows.len() {
                            for dx in 0..samples_per_tile_dim {
                                let col_idx = ((cx - cell_min_x) as usize * pixels_per_cell) + (sx * samples_per_tile_dim + dx);
                                
                                if col_idx < t_width {
                                    // Texture
                                    let tex = texture_rows[dy][col_idx];
                                    if let Some(&form_id) = texture_mapping.get(&tex) {
                                        *texture_votes.entry(form_id).or_insert(0) += 1;
                                    }

                                    // Water vs Land comparison
                                    if dy < water_rows.len() && dy < hmap_rows.len() && (col_idx * 2 + 1) < water_rows[dy].len() && (col_idx * 2 + 1) < hmap_rows[dy].len() {
                                        let wh = u16::from_le_bytes(water_rows[dy][col_idx*2..col_idx*2+2].try_into().unwrap());
                                        let lh = u16::from_le_bytes(hmap_rows[dy][col_idx*2..col_idx*2+2].try_into().unwrap());
                                        
                                        if (wh as i32) > (lh as i32 + 5) { // 5 unit buffer for precision
                                            water_count += 1;
                                        }
                                    }
                                }
                                total_pixels += 1;
                            }
                        }

                        if total_pixels == 0 { continue; }

                        // Majority Vote
                        let has_water = water_count > (total_pixels / 3); 
                        let best_texture = texture_votes.into_iter()
                            .max_by_key(|&(_, count)| count)
                            .map(|(id, _)| id);

                        if has_water || best_texture.is_some() {
                            let mut type_str = "Misc";
                            if has_water {
                                type_str = "Water";
                            } else if let Some(form_id) = best_texture {
                                if let Some(texture_name) = ltex_map.get(&form_id) {
                                    let id_lower = texture_name.to_lowercase();
                                    if id_lower.contains("grass") || id_lower.contains("dirt") || id_lower.contains("forest") {
                                        type_str = "Grass";
                                    } else if id_lower.contains("water") || id_lower.contains("sea") || id_lower.contains("river") {
                                        type_str = "Water";
                                    } else if id_lower.contains("sand") || id_lower.contains("beach") {
                                        type_str = "Sand";
                                    } else if id_lower.contains("rock") || id_lower.contains("stone") || id_lower.contains("cliff") {
                                        type_str = "Rock";
                                    } else if id_lower.contains("city") || id_lower.contains("road") || id_lower.contains("asphalt") || id_lower.contains("rubble") {
                                        type_str = "City";
                                    }
                                }
                            }

                            if type_str != "Misc" {
                                tiles.insert((cx * tiles_per_dimension as i32 + sx as i32, cy * tiles_per_dimension as i32 + sy as i32), RawTile {
                                    x: cx * tiles_per_dimension as i32 + sx as i32,
                                    y: cy * tiles_per_dimension as i32 + sy as i32,
                                    tile_type: type_str.to_string(),
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
        }
        
        let _ = std::fs::remove_file(&terrain_dds);
        let _ = std::fs::remove_file(&water_dds);
        let _ = std::fs::remove_file(&hmap_dds);

        let world_width = (world_cells_x * tiles_per_dimension) as u32;
        let world_height = (world_cells_y * tiles_per_dimension) as u32;

        Ok((tiles, world_width, world_height))
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

        let plugin_paths = self.resolve_plugins(&install_path).unwrap_or_else(|e| {
            warn!("Failed to resolve plugins: {}", e);
            vec![]
        });

        if plugin_paths.is_empty() {
             return Err(io::Error::new(io::ErrorKind::NotFound, "No plugins resolved."));
        }

        let ltex_map = self.run_esmdump_ltex(&plugin_paths).unwrap_or_default();

        // 0. Resolve World ID
        let world_id = if self.worldspace.to_lowercase() == "commonwealth" { "0x0000003C" } else { &self.worldspace };

        // 1. Metadata (Markers / Trees / Rocks / Bounds)
        let (locations, _trees, _rocks, plugin_bounds) = self.run_esmdump_refrs(&install_path, &plugin_paths, world_id)?;
        
        let world_id_clean = world_id.trim_start_matches("0x").to_uppercase();
        // 2. Terrain and Water (fo4land/findwater)
        let mut warnings = vec![];
        let (mut tiles, world_width, world_height) = self.run_fo4land_and_findwater(&install_path, &plugin_paths, &ltex_map, &mut warnings).unwrap_or_else(|e| {
            warn!("Failed to extract terrain/water: {}", e);
            (std::collections::HashMap::new(), 512, 512)
        });

        // 3. Apply Density Overlays (Trees / Rocks)
        let cell_size = 4096.0;
        let tiles_per_dimension = 32; // Matches fo4land -z 32
        let units_per_tile = cell_size / tiles_per_dimension as f32;

        for (tx, ty) in _trees {
            let gx = (tx as f32 / units_per_tile as f32).floor() as i32;
            let gy = (ty as f32 / units_per_tile as f32).floor() as i32;
            
            tiles.entry((gx, gy)).and_modify(|t| {
                if t.tile_type == "Misc" {
                    t.tile_type = "Forest".to_string();
                }
            }).or_insert_with(|| RawTile {
                x: gx,
                y: gy,
                tile_type: "Forest".to_string(),
                real_lat: None,
                real_lon: None,
                weight: 1.0,
                anchor: None,
            });
        }

        for (rx, ry) in _rocks {
            let gx = (rx as f32 / units_per_tile as f32).floor() as i32;
            let gy = (ry as f32 / units_per_tile as f32).floor() as i32;
            
            tiles.entry((gx, gy)).and_modify(|t| {
                if t.tile_type == "Misc" || t.tile_type == "Forest" {
                    t.tile_type = "Mountain".to_string();
                }
            }).or_insert_with(|| RawTile {
                x: gx,
                y: gy,
                tile_type: "Mountain".to_string(),
                real_lat: None,
                real_lon: None,
                weight: 1.0,
                anchor: None,
            });
        }

        let scaled_bounds = plugin_bounds.map(|b| MapBounds {
            min_x: b.min_x * tiles_per_dimension as i32,
            min_y: b.min_y * tiles_per_dimension as i32,
            max_x: (b.max_x + 1) * tiles_per_dimension as i32 - 1,
            max_y: (b.max_y + 1) * tiles_per_dimension as i32 - 1,
        });

        // 4. Return the data
        let mut data = RawMapData {
            locations,
            tiles: tiles.into_values().collect(),
            width: world_width,
            height: world_height,
            coordinate_system: crate::schema::CoordinateSystem::Absolute,
            rows: None,
            world_bounds: scaled_bounds,
            has_grid: true,
            coordinate_scale: 512.0, 
            warnings,
            priority: self.variant.priority(),
            ..Default::default()
        };
        Ok(data)
    }
}
