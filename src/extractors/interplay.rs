use crate::schema::{RawMapData, RawLocation, RawTile, IconType};
use crate::extractors::Extractor;
use crate::dat_extractor;
use std::io;
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use log::{info, error, warn};

const FO1IN2_BASE_URL: &str = "https://raw.githubusercontent.com/rotators/Fo1in2/master/Fallout2/Fallout1in2/mods/fo1_base";
/// World-map geometry shared by FO1 (Fo1in2) and FO2: squares are 50 px; art tiles are
/// 350x300 px (7x6 squares), 4 per row.
const SQUARE_PX: usize = 50;
const TILE_PX_W: usize = 350;
const TILE_PX_H: usize = 300;
const TILE_COLS: usize = 4;
/// Size of the refined terrain blocks emitted from the art (5x5 per square).
const SUB_PX: usize = 10;

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

        // 5. Refine coastal squares from the world-map art (falls back to whole squares).
        let mut warnings = Vec::new();
        let (tiles, width, height, scale) = match self.load_world_art(&install_dir, width as usize, height as usize) {
            Ok((rgb, mask)) => {
                let refined = refine_with_art(&tiles, width as usize, height as usize, &rgb, &mask);
                let k = (SQUARE_PX / SUB_PX) as u32;
                (refined, width * k, height * k, SUB_PX as f32)
            }
            Err(e) => {
                let msg = format!("{}: world-map art unavailable ({}); terrain stays at whole-square resolution", self.variant.name(), e);
                warn!("{}", msg);
                warnings.push(msg);
                (tiles, width, height, SQUARE_PX as f32)
            }
        };

        info!("Final Extracted: {} tiles, {} locations", tiles.len(), locations.len());

        Ok(RawMapData {
            locations,
            tiles,
            width: width as u32,
            height: height as u32,
            coordinate_system: crate::schema::CoordinateSystem::Absolute,
            rows: None,
            coordinate_scale: scale, // pixels per tile; derived Default would give 0.0
            world_bounds: None,
            warnings,
            ..Default::default()
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
            let url = format!("{}/data/{}", FO1IN2_BASE_URL, filename.replace('\\', "/"));
            
            info!("[Downloader] fetching {}...", url);
            let response = ureq::get(&url).call().map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Download failed for {}: {}", url, e)))?;
            let s = response.into_string()?;
            if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
            fs::write(&cache_path, &s)?;
            return Ok(s);
        }
        
        Err(io::Error::new(io::ErrorKind::NotFound, format!("Text file {} not found (Variant: {:?})", filename, self.variant)))
    }

    /// Loads a binary game file by its path relative to the game root (e.g. "art/intrface/WRLDMP00.FRM"),
    /// trying the cache, a loose file, master.dat (FO2), then the Fo1in2 repository (FO1).
    fn load_binary_file(&self, install_dir: &Option<PathBuf>, path: &str) -> io::Result<Vec<u8>> {
        let path = path.replace('\\', "/");
        let cache_path = Path::new(".cache").join(self.variant.id_str()).join(&path);
        if cache_path.exists() {
            return fs::read(&cache_path);
        }
        let cache = |data: Vec<u8>| -> io::Result<Vec<u8>> {
            if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
            fs::write(&cache_path, &data)?;
            Ok(data)
        };

        if let Some(dir) = install_dir {
            let loose = dir.join(&path);
            if loose.exists() {
                return cache(fs::read(loose)?);
            }
            if let GameVariant::Fallout2 = self.variant {
                let master_dat = dir.join("master.dat");
                if master_dat.exists() {
                    if let Ok(data) = dat_extractor::extract_dat2(&master_dat, &path.replace('/', "\\")) {
                        return cache(data);
                    }
                }
            }
        }

        if let GameVariant::Fallout1 = self.variant {
            let url = format!("{}/{}", FO1IN2_BASE_URL, path);
            info!("[Downloader] fetching {}...", url);
            let response = ureq::get(&url).call()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Download failed for {}: {}", url, e)))?;
            let mut data = Vec::new();
            response.into_reader().read_to_end(&mut data)?;
            return cache(data);
        }

        Err(io::Error::new(io::ErrorKind::NotFound, format!("Binary file {} not found (Variant: {:?})", path, self.variant)))
    }

    /// The world-map art as one RGB image (TILE_COLS*350 x rows*300), plus FO2's walk masks
    /// (`Some(true)` = sea) where a mask file exists.
    fn load_world_art(&self, install_dir: &Option<PathBuf>, width_sq: usize, height_sq: usize)
        -> io::Result<(Vec<[u8; 3]>, Vec<Option<bool>>)>
    {
        let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_string());
        // Fo1in2 runs on the FO2 engine, so FO1's art uses FO2's palette.
        let pal_bytes = match self.variant {
            GameVariant::Fallout2 => self.load_binary_file(install_dir, "color.pal")?,
            _ => {
                let cache_path = Path::new(".cache").join(self.variant.id_str()).join("color.pal");
                if cache_path.exists() { fs::read(&cache_path)? } else {
                    let fo2 = crate::utils::load_config_paths().unwrap_or_default().get("fallout2").map(PathBuf::from)
                        .or_else(|| dat_extractor::find_steam_app(GameVariant::Fallout2.app_id(), GameVariant::Fallout2.name()).ok())
                        .ok_or_else(|| bad("FO1 art needs FO2's color.pal, but Fallout 2 is not installed"))?;
                    let data = dat_extractor::extract_dat2(&fo2.join("master.dat"), "color.pal")?;
                    if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
                    fs::write(&cache_path, &data)?;
                    data
                }
            }
        };
        let pal = crate::frm::palette(&pal_bytes).ok_or_else(|| bad("color.pal too short"))?;

        let (w, h) = (width_sq * SQUARE_PX, height_sq * SQUARE_PX);
        let mut rgb = vec![[0u8; 3]; w * h];
        let mut mask = vec![None; w * h];
        let tile_rows = h / TILE_PX_H;
        for t in 0..TILE_COLS * tile_rows {
            let (ox, oy) = ((t % TILE_COLS) * TILE_PX_W, (t / TILE_COLS) * TILE_PX_H);
            let frm = self.load_binary_file(install_dir, &format!("art/intrface/WRLDMP{:02}.FRM", t))?;
            let (fw, fh, px) = crate::frm::decode_frm(&frm).ok_or_else(|| bad("bad world-map FRM"))?;
            if (fw, fh) != (TILE_PX_W, TILE_PX_H) { return Err(bad("unexpected world-map tile size")); }
            for y in 0..fh { for x in 0..fw {
                rgb[(oy + y) * w + ox + x] = pal[px[y * fw + x] as usize];
            }}
            // Only FO2 ships real masks (Fo1in2's are all empty); missing files just mean "no mask".
            if let GameVariant::Fallout2 = self.variant {
                if let Ok(m) = self.load_binary_file(install_dir, &format!("data/WRLDMP{:02}.MSK", t)) {
                    let row_bytes = TILE_PX_W.div_ceil(8);
                    for y in 0..TILE_PX_H { for x in 0..TILE_PX_W {
                        if let Some(byte) = m.get(y * row_bytes + x / 8) {
                            mask[(oy + y) * w + ox + x] = Some((byte >> (7 - x % 8)) & 1 == 1);
                        }
                    }}
                }
            }
        }
        Ok((rgb, mask))
    }

    fn remap_location_name(&self, name: &str) -> String {
        let canonical = match (self.variant, name) {
            (GameVariant::Fallout1, "Military Base") => "Mariposa Military Base",
            (GameVariant::Fallout1, "Glow") => "The Glow",
            (GameVariant::Fallout1, "Hub") => "The Hub",
            (GameVariant::Fallout1, "Boneyard") => "The Boneyard",
            (GameVariant::Fallout1, "Cathedral") => "The Cathedral",
            (GameVariant::Fallout1, "Brotherhood of Steel") => "Lost Hill Bunker",
            (GameVariant::Fallout2, "Military Base") => "Mariposa Military Base",
            (GameVariant::Fallout2, "NCR") => "Shady Sands", // NCR's capital is Shady Sands
            (GameVariant::Fallout2, "Den") => "The Den",
            (GameVariant::Fallout2, "Enclave") => "Poseidon Energy Oil Rig",
            (GameVariant::Fallout2, "abbey") => "Abbey",
            (GameVariant::Fallout2, "EPA") => "Environmental Protection Agency San Francisco Facility",
            _ => name,
        };
        canonical.to_string()
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
        const TILE_WIDTH: u32 = (TILE_PX_W / SQUARE_PX) as u32;
        const TILE_HEIGHT: u32 = (TILE_PX_H / SQUARE_PX) as u32;
        const NUM_HORIZONTAL_TILES: u32 = TILE_COLS as u32;
        
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
            has_grid: true,
            coordinate_scale: 50.0,
            reference_lon: None,
            warnings: vec![],
            terrain_data: None,
            world_bounds: None,
            ..Default::default()
        }
    }
}

/// Box mean of `v` (w x h) over a (2r+1)^2 window, clamped at the edges, via an integral image.
fn box_mean(v: &[f64], w: usize, h: usize, r: usize) -> Vec<f64> {
    let mut sat = vec![0.0; (w + 1) * (h + 1)];
    for y in 0..h { for x in 0..w {
        sat[(y + 1) * (w + 1) + x + 1] = v[y * w + x] + sat[y * (w + 1) + x + 1] + sat[(y + 1) * (w + 1) + x] - sat[y * (w + 1) + x];
    }}
    let mut out = vec![0.0; w * h];
    for y in 0..h { for x in 0..w {
        let (x0, y0, x1, y1) = (x.saturating_sub(r), y.saturating_sub(r), (x + r + 1).min(w), (y + r + 1).min(h));
        let s = sat[y1 * (w + 1) + x1] - sat[y0 * (w + 1) + x1] - sat[y1 * (w + 1) + x0] + sat[y0 * (w + 1) + x0];
        out[y * w + x] = s / ((x1 - x0) * (y1 - y0)) as f64;
    }}
    out
}

/// Per-pixel sea/land from the art. The classifier is trained on the game's own interior squares
/// (a square whose 8 neighbours share its type), so each game's art style teaches itself: features are
/// the 9x9 mean colour and brightness texture; the decision is a linear discriminant (LDA).
/// None if the game has no interior sea or land squares to learn from.
fn classify_sea(rgb: &[[u8; 3]], w: usize, h: usize, is_water: &dyn Fn(i32, i32) -> Option<bool>) -> Option<Vec<bool>> {
    const R: usize = 4;
    let chan = |c: usize| rgb.iter().map(|p| p[c] as f64).collect::<Vec<_>>();
    let lum: Vec<f64> = rgb.iter().map(|p| (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0).collect();
    let lum2: Vec<f64> = lum.iter().map(|l| l * l).collect();
    let (m_lum, m_lum2) = (box_mean(&lum, w, h, R), box_mean(&lum2, w, h, R));
    let feats: [Vec<f64>; 4] = [
        box_mean(&chan(0), w, h, R), box_mean(&chan(1), w, h, R), box_mean(&chan(2), w, h, R),
        m_lum.iter().zip(&m_lum2).map(|(m, m2)| (m2 - m * m).max(0.0).sqrt()).collect(),
    ];
    let f = |i: usize| [feats[0][i], feats[1][i], feats[2][i], feats[3][i]];

    // Training pixels: interior squares only.
    let (mut sum, mut n) = ([[0.0f64; 4]; 2], [0usize; 2]);
    let mut samples: Vec<(usize, usize)> = Vec::new(); // (pixel, class)
    for sy in 0..(h / SQUARE_PX) as i32 { for sx in 0..(w / SQUARE_PX) as i32 {
        let Some(water) = is_water(sx, sy) else { continue };
        let interior = (-1..=1).all(|dy| (-1..=1).all(|dx| is_water(sx + dx, sy + dy).map_or(true, |v| v == water)));
        if !interior { continue; }
        let class = water as usize;
        for y in 0..SQUARE_PX { for x in 0..SQUARE_PX {
            let i = (sy as usize * SQUARE_PX + y) * w + sx as usize * SQUARE_PX + x;
            let v = f(i);
            for k in 0..4 { sum[class][k] += v[k]; }
            n[class] += 1;
            samples.push((i, class));
        }}
    }}
    if n[0] == 0 || n[1] == 0 { return None; }
    let mu = |c: usize| { let mut m = sum[c]; for k in 0..4 { m[k] /= n[c] as f64; } m };
    let (mu0, mu1) = (mu(0), mu(1));
    let total = (n[0] + n[1]) as f64;
    let mean: Vec<f64> = (0..4).map(|k| (sum[0][k] + sum[1][k]) / total).collect();
    let mut cov = [[0.0f64; 4]; 4];
    for &(i, _) in &samples {
        let v = f(i);
        for a in 0..4 { for b in 0..4 { cov[a][b] += (v[a] - mean[a]) * (v[b] - mean[b]) / total; } }
    }
    let wv = solve4(cov, std::array::from_fn(|k| mu1[k] - mu0[k]))?;
    let dot = |v: [f64; 4]| (0..4).map(|k| v[k] * wv[k]).sum::<f64>();
    let thr = (dot(mu0) + dot(mu1)) / 2.0;
    Some((0..w * h).map(|i| dot(f(i)) > thr).collect())
}

/// Solves the 4x4 system A x = b (Gaussian elimination, partial pivoting).
fn solve4(mut a: [[f64; 4]; 4], mut b: [f64; 4]) -> Option<[f64; 4]> {
    for c in 0..4 {
        let p = (c..4).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c].abs() < 1e-12 { return None; }
        a.swap(c, p); b.swap(c, p);
        for r in c + 1..4 {
            let k = a[r][c] / a[c][c];
            for j in c..4 { a[r][j] -= k * a[c][j]; }
            b[r] -= k * b[c];
        }
    }
    let mut x = [0.0; 4];
    for r in (0..4).rev() { x[r] = (b[r] - (r + 1..4).map(|j| a[r][j] * x[j]).sum::<f64>()) / a[r][r]; }
    Some(x)
}

/// Splits each square into SUB_PX blocks. Coastal squares (Water, or touching Water) take Water or
/// land per block from the art: FO2's walk mask where present, else the learned classifier.
/// Inland squares keep their own type, so the art can never invent lakes.
fn refine_with_art(squares: &[RawTile], w_sq: usize, h_sq: usize, rgb: &[[u8; 3]], mask: &[Option<bool>]) -> Vec<RawTile> {
    let (w, h) = (w_sq * SQUARE_PX, h_sq * SQUARE_PX);
    let ty: HashMap<(i32, i32), &str> = squares.iter().map(|t| ((t.x, t.y), t.tile_type.as_str())).collect();
    let is_water = |x: i32, y: i32| ty.get(&(x, y)).map(|t| *t == "Water");
    let learned = classify_sea(rgb, w, h, &is_water);
    let sea = |i: usize| mask[i].or_else(|| learned.as_ref().map(|l| l[i]));

    let k = SQUARE_PX / SUB_PX;
    let mut out = Vec::with_capacity(squares.len() * k * k);
    for sq in squares {
        let (sx, sy) = (sq.x, sq.y);
        let neighbours = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (sx + dx, sy + dy)));
        let coastal = neighbours.clone().any(|(x, y)| is_water(x, y) == Some(true));
        // Land type for sea-square blocks the art shows as land: the most common land neighbour.
        let land = {
            let mut votes: HashMap<&str, usize> = HashMap::new();
            for (x, y) in neighbours { if let Some(t) = ty.get(&(x, y)) { if *t != "Water" { *votes.entry(t).or_default() += 1; } } }
            votes.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0))).map_or("Sand", |(t, _)| t)
        };
        for by in 0..k { for bx in 0..k {
            let tile_type = if coastal {
                let (mut s, mut n) = (0usize, 0usize);
                for y in 0..SUB_PX { for x in 0..SUB_PX {
                    let i = (sy as usize * SQUARE_PX + by * SUB_PX + y) * w + sx as usize * SQUARE_PX + bx * SUB_PX + x;
                    if let Some(v) = sea(i) { n += 1; s += v as usize; }
                }}
                match (n > 0, 2 * s > n) {
                    (true, true) => "Water",
                    (true, false) if sq.tile_type == "Water" => land,
                    _ => sq.tile_type.as_str(),
                }
            } else { sq.tile_type.as_str() };
            out.push(RawTile {
                x: sx * k as i32 + bx as i32, y: sy * k as i32 + by as i32,
                tile_type: tile_type.to_string(),
                real_lat: None, real_lon: None, weight: sq.weight, anchor: None,
            });
        }}
    }
    debug_assert_eq!(h, h_sq * SQUARE_PX);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The learned sea classifier must agree with FO2's exact walk masks on coastal pixels,
    /// since FO1 (which has no masks) relies on it alone.
    #[test]
    fn learned_sea_matches_fo2_masks() {
        let ex = FalloutInterplayExtractor::new(GameVariant::Fallout2);
        let dir = dat_extractor::find_steam_app(GameVariant::Fallout2.app_id(), "Fallout 2").ok();
        let wm = ex.load_text_file(&dir, "WORLDMAP.TXT").unwrap();
        let (squares, ws, hs, _) = ex.parse_worldmap_txt(&wm).unwrap();
        let (rgb, mask) = match ex.load_world_art(&dir, ws as usize, hs as usize) {
            Ok(a) => a,
            Err(e) => { eprintln!("skipping: FO2 art unavailable ({})", e); return; }
        };
        let (w, h) = (ws as usize * SQUARE_PX, hs as usize * SQUARE_PX);
        let ty: HashMap<(i32, i32), bool> = squares.iter().map(|t| ((t.x, t.y), t.tile_type == "Water")).collect();
        let learned = classify_sea(&rgb, w, h, &|x, y| ty.get(&(x, y)).copied()).expect("classifier");
        let (mut agree, mut total) = (0usize, 0usize);
        for y in 0..h { for x in 0..w {
            let (sx, sy) = ((x / SQUARE_PX) as i32, (y / SQUARE_PX) as i32);
            let coastal = (-1..=1).any(|dy| (-1..=1).any(|dx| ty.get(&(sx + dx, sy + dy)) == Some(&true)));
            if let (true, Some(m)) = (coastal, mask[y * w + x]) {
                total += 1;
                agree += (m == learned[y * w + x]) as usize;
            }
        }}
        let acc = agree as f64 / total as f64;
        println!("learned vs mask on {} coastal pixels: {:.3}", total, acc);
        assert!(acc > 0.9, "classifier accuracy {:.3}", acc);
    }
}
