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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GameVariant {
    Fallout1,
    Fallout2,
    /// Fallout 2–engine total conversions (tier 5), fetched by `crate::mods`. Their markers only
    /// anchor the warp; they place terrain, not markers.
    FalloutNevada,
    FalloutSonora,
}

impl GameVariant {
    fn app_id(&self) -> u32 {
        match self {
            GameVariant::Fallout1 => 38400,
            GameVariant::Fallout2 => 38410,
            GameVariant::FalloutNevada | GameVariant::FalloutSonora => 0, // mods: not on Steam
        }
    }

    fn name(&self) -> &str {
        match self {
            GameVariant::Fallout1 => "Fallout 1",
            GameVariant::Fallout2 => "Fallout 2",
            GameVariant::FalloutNevada => "Fallout: Nevada",
            GameVariant::FalloutSonora => "Fallout: Sonora",
        }
    }
    
    fn id_str(&self) -> &str {
        match self {
            GameVariant::Fallout1 => "fallout1",
            GameVariant::Fallout2 => "fallout2",
            GameVariant::FalloutNevada => "fallout_nevada",
            GameVariant::FalloutSonora => "fallout_sonora",
        }
    }

    fn is_mod(&self) -> bool {
        matches!(self, GameVariant::FalloutNevada | GameVariant::FalloutSonora)
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
        if self.variant.is_mod() { format!("mod_{}", self.variant.id_str()) } else { "interplay".to_string() }
    }

    /// Mods: the manifest (their files are fetched and verified against it).
    fn get_source_paths(&self) -> Vec<String> {
        if self.variant.is_mod() { vec!["input/config/mods.json".to_string()] } else { Vec::new() }
    }

    fn extract(&self) -> RawMapData {
        match self.extract_real_data() {
            Ok(data) => data,
            Err(e) if self.variant.is_mod() => {
                let msg = format!("{} skipped: {}", self.variant.name(), e);
                warn!("{}", msg);
                RawMapData { warnings: vec![msg], coordinate_scale: SQUARE_PX as f32, priority: 5, ..Default::default() }
            }
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
            install_dir = if self.variant.is_mod() {
                // The fetched mod's game folder: the one holding its master.dat, or its patch000.dat
                // when it installs over Fallout 2 (Nevada).
                let files = crate::mods::fetch(self.variant.id_str()).map_err(io::Error::other)?;
                Some(find_dir_with(&files, "master.dat").or_else(|| find_dir_with(&files, "patch000.dat"))
                    .ok_or_else(|| io::Error::other("no master.dat or patch000.dat in the mod's files"))?)
            } else {
                dat_extractor::find_steam_app(self.variant.app_id(), self.variant.name()).ok()
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
        // Mods reuse Fallout 2's internal area names for their own places; the names the player sees
        // are map.msg's entries 1500 + area index.
        let area_names = if self.variant.is_mod() {
            let msg = self.load_binary_file(&install_dir, "text/english/game/map.msg")?;
            Some(parse_msg(&String::from_utf8_lossy(&msg)))
        } else {
            None
        };
        let locations = self.parse_city_txt(&city_txt, area_names.as_ref())?;
        


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
            priority: if self.variant.is_mod() { 5 } else { 1 },
            anchor_only_markers: self.variant.is_mod(),
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
        
        // Try Local override or discovered directory, in the engine's order: patch DATs, loose files,
        // then the main DATs.
        if let Some(dir) = install_dir {
            let inner_path = format!("data/{}", filename.replace('/', "\\"));
            let (patches, mains) = self.dats(dir);
            let from_dats = |dats: &[PathBuf]| dats.iter().find_map(|dat| dat_extractor::extract_dat2(dat, &inner_path).ok());
            if let Some(bytes) = from_dats(&patches) {
                let s = String::from_utf8_lossy(&bytes).to_string();
                if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
                fs::write(&cache_path, &s)?;
                return Ok(s);
            }

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
            
            // 3. Main DATs (Fallout 2–engine games)
            if let Some(bytes) = from_dats(&mains) {
                let s = String::from_utf8_lossy(&bytes).to_string();
                if let Some(parent) = cache_path.parent() { fs::create_dir_all(parent)?; }
                fs::write(&cache_path, &s)?;
                return Ok(s);
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
            // The engine's order: patch DATs, loose files, then the main DATs.
            let (patches, mains) = self.dats(dir);
            let from_dats = |dats: &[PathBuf]| dats.iter().find_map(|dat| dat_extractor::extract_dat2(dat, &path.replace('/', "\\")).ok());
            if let Some(data) = from_dats(&patches) {
                return cache(data);
            }
            let loose = dir.join(&path);
            if loose.exists() {
                return cache(fs::read(loose)?);
            }
            if let Some(data) = from_dats(&mains) {
                return cache(data);
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
            GameVariant::Fallout2 | GameVariant::FalloutNevada | GameVariant::FalloutSonora => self.load_binary_file(install_dir, "color.pal")?,
            GameVariant::Fallout1 => {
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
            if self.variant != GameVariant::Fallout1 {
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

    /// The DAT archives of a Fallout 2–engine game folder, as the engine searches them: the patch
    /// DATs (patch000.dat...), which override loose files, then master.dat and any other DATs
    /// (e.g. Sonora's fission.dat), which loose files override. Fallout 1 (Fo1in2) has none.
    /// A mod installed over Fallout 2 falls back to Fallout 2's master.dat.
    fn dats(&self, dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
        if self.variant == GameVariant::Fallout1 { return (Vec::new(), Vec::new()); }
        let mut all: Vec<PathBuf> = fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dat")))
            .collect();
        all.sort();
        let name = |p: &PathBuf| p.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
        let (mut patches, mut mains): (Vec<PathBuf>, Vec<PathBuf>) = all.into_iter().partition(|p| name(p).starts_with("patch"));
        // The engine opens patch000.dat, patch001.dat... in turn, each searched before the last.
        patches.sort_by_key(|p| std::cmp::Reverse(name(p)));
        mains.sort_by_key(|p| name(p) != "master.dat"); // master.dat first
        // A mod installed over Fallout 2 (no master.dat of its own) reads Fallout 2's.
        if self.variant.is_mod() && !mains.iter().any(|p| name(p) == "master.dat") {
            let fo2 = crate::utils::load_config_paths().unwrap_or_default().get("fallout2").map(PathBuf::from)
                .or_else(|| dat_extractor::find_steam_app(GameVariant::Fallout2.app_id(), GameVariant::Fallout2.name()).ok());
            mains.extend(fo2.map(|d| d.join("master.dat")).filter(|p| p.exists()));
        }
        (patches, mains)
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

    /// Category of every world-map location. Major sites (towns, vaults, landmarks) appear on
    /// the main map; minor or random-encounter locations are `Other` (shown on the full map only).
    fn resolve_marker_type(name: &str) -> IconType {
        match name {
            "Vault 13" | "Vault 15" => IconType::Vault,
            // Towns
            "Shady Sands" | "Junktown" | "The Hub" | "Necropolis" | "The Boneyard"
            | "Arroyo" | "The Den" | "Klamath" | "Modoc" | "Gecko" | "Broken Hills" | "New Reno"
            | "Redding" | "San Francisco" | "Vault City" => IconType::City,
            "Primitive Tribe" => IconType::Settlement,
            // Bases, facilities and other major sites
            "Lost Hill Bunker" | "Mariposa Military Base" | "The Glow" | "The Cathedral" | "Navarro"
            | "Sierra Army Base" | "Poseidon Energy Oil Rig" | "Abbey"
            | "Environmental Protection Agency San Francisco Facility" => IconType::Landmark,
            // Everything else: minor locations, side areas and map variants (e.g. Hub Deathclaw Cave,
            // Ghost Farm, Golgotha, Reno Stables, Fake Vault 13a/b).
            _ => IconType::Other,
        }
    }

    fn parse_city_txt(&self, content: &str, area_names: Option<&HashMap<u32, String>>) -> io::Result<Vec<RawLocation>> {
        let mut locations = Vec::new();
        let mut area: Option<u32> = None;
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
                    let shown = area.and_then(|a| area_names?.get(&(1500 + a)).cloned());
                    self.add_location_if_valid(&mut locations, name, shown, x, y);
                }
                current_name = None;
                current_pos = None;
                area = clean_line[1..clean_line.len() - 1].trim().strip_prefix("Area").and_then(|n| n.trim().parse().ok());
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
            let shown = area.and_then(|a| area_names?.get(&(1500 + a)).cloned());
            self.add_location_if_valid(&mut locations, name, shown, x, y);
        }
        
        info!("Extracted {} locations from CITY.TXT", locations.len());
        Ok(locations)
    }

    /// Adds area `name` (its CITY.TXT name, which decides whether it is a real place) as `shown`
    /// when given (its in-game name); an area listed again under a name already added (an alternate
    /// state of the same place) is skipped.
    fn add_location_if_valid(&self, locations: &mut Vec<RawLocation>, name: String, shown: Option<String>, x: f32, y: f32) {
        // (100, 100) is the engine's placeholder for areas placed at run time or unused slots.
        if (x, y) == (100.0, 100.0) { return; }
        let name_lower = name.to_lowercase();
        let invalid = ["random", "shadow worlds", "cathedraldead", "vatsdead", "raiders", 
                      "shady sands radscorpion cave", "hub irwines farm", "car outta gas", 
                      "vipers", "scrapheap", "desert", "mountain", "ocean"];
        
        if !(invalid.iter().any(|&s| name_lower.contains(s)) || name_lower.starts_with("special")) && !name_lower.is_empty() {
             let final_name = shown.unwrap_or_else(|| self.remap_location_name(&name));
             if locations.iter().any(|l| l.name == final_name) { return; }
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

/// Per-pixel terrain classifier learned from the art. It is trained on the game's own interior
/// squares (a square whose 8 neighbours share its type), so each game's art style teaches itself.
/// Features: 9x9 mean colour and brightness texture. Decision: linear discriminant (LDA) with one
/// shared covariance, restricted at prediction time to the types allowed at that spot.
struct ArtModel {
    feats: [Vec<f64>; 4],
    classes: Vec<String>,
    means: Vec<[f64; 4]>,
    inv_cov: [[f64; 4]; 4],
}

impl ArtModel {
    /// None if fewer than two square types have interior squares to learn from.
    /// `train_on` limits which squares are used for training (all of them in production).
    fn train(rgb: &[[u8; 3]], w: usize, h: usize, ty: &HashMap<(i32, i32), String>, train_on: &dyn Fn(i32, i32) -> bool) -> Option<Self> {
        const R: usize = 4;
        let chan = |c: usize| rgb.iter().map(|p| p[c] as f64).collect::<Vec<_>>();
        let lum: Vec<f64> = rgb.iter().map(|p| (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0).collect();
        let lum2: Vec<f64> = lum.iter().map(|l| l * l).collect();
        let (m_lum, m_lum2) = (crate::raster::box_mean(&lum, w, h, R), crate::raster::box_mean(&lum2, w, h, R));
        let feats: [Vec<f64>; 4] = [
            crate::raster::box_mean(&chan(0), w, h, R), crate::raster::box_mean(&chan(1), w, h, R), crate::raster::box_mean(&chan(2), w, h, R),
            m_lum.iter().zip(&m_lum2).map(|(m, m2)| (m2 - m * m).max(0.0).sqrt()).collect(),
        ];
        let f = |i: usize| [feats[0][i], feats[1][i], feats[2][i], feats[3][i]];

        let mut classes: Vec<String> = ty.values().cloned().collect();
        classes.sort();
        classes.dedup();
        let (mut sum, mut n) = (vec![[0.0f64; 4]; classes.len()], vec![0usize; classes.len()]);
        let mut samples: Vec<(usize, usize)> = Vec::new();
        let is_interior = |sx: i32, sy: i32, t: &String| (-1..=1).all(|dy| (-1..=1).all(|dx| ty.get(&(sx + dx, sy + dy)).map_or(true, |v| v == t)));
        // Types too small to have interior squares (towns) learn from all their squares.
        let has_interior: Vec<&String> = ty.iter().filter(|(p, t)| is_interior(p.0, p.1, t)).map(|(_, t)| t).collect();
        for (&(sx, sy), t) in ty {
            if !train_on(sx, sy) { continue; }
            if has_interior.contains(&t) && !is_interior(sx, sy, t) { continue; }
            let k = classes.iter().position(|c| c == t).unwrap();
            for y in 0..SQUARE_PX { for x in 0..SQUARE_PX {
                let i = (sy as usize * SQUARE_PX + y) * w + sx as usize * SQUARE_PX + x;
                let v = f(i);
                for d in 0..4 { sum[k][d] += v[d]; }
                n[k] += 1;
                samples.push((i, k));
            }}
        }
        // Pooled within-class covariance, then keep only classes that had training squares.
        let mean = |k: usize| -> [f64; 4] { std::array::from_fn(|d| sum[k][d] / n[k] as f64) };
        let mut cov = [[0.0f64; 4]; 4];
        for &(i, k) in &samples {
            let (mu, v) = (mean(k), f(i));
            for a in 0..4 { for b in 0..4 { cov[a][b] += (v[a] - mu[a]) * (v[b] - mu[b]) / samples.len() as f64; } }
        }
        let kept: Vec<usize> = (0..classes.len()).filter(|&k| n[k] > 0).collect();
        if kept.len() < 2 { return None; }
        let means: Vec<[f64; 4]> = kept.iter().map(|&k| mean(k)).collect();
        let classes: Vec<String> = kept.iter().map(|&k| classes[k].clone()).collect();
        let mut inv_cov = [[0.0; 4]; 4];
        for c in 0..4 {
            let col = solve4(cov, std::array::from_fn(|r| (r == c) as u8 as f64))?;
            for r in 0..4 { inv_cov[r][c] = col[r]; }
        }
        Some(ArtModel { feats, classes, means, inv_cov })
    }

    fn class_index(&self, name: &str) -> Option<usize> { self.classes.iter().position(|c| c == name) }

    /// Most likely class of pixel `i` among `allowed` (class indices); None if none allowed.
    fn predict(&self, i: usize, allowed: &[usize]) -> Option<usize> {
        let x: [f64; 4] = std::array::from_fn(|d| self.feats[d][i]);
        let score = |k: usize| {
            let mu = self.means[k];
            let s: [f64; 4] = std::array::from_fn(|r| (0..4).map(|c| self.inv_cov[r][c] * mu[c]).sum());
            (0..4).map(|d| x[d] * s[d] - 0.5 * mu[d] * s[d]).sum::<f64>()
        };
        allowed.iter().copied().max_by(|&a, &b| score(a).total_cmp(&score(b)))
    }
}

/// Two-stage art classifier: water vs land is one binary decision (sharper than a shared
/// multi-class fit); land types are then separated by their own model.
struct ArtClassifier { water: Option<ArtModel>, land: Option<ArtModel> }

impl ArtClassifier {
    fn train(rgb: &[[u8; 3]], w: usize, h: usize, ty: &HashMap<(i32, i32), String>, train_on: &dyn Fn(i32, i32) -> bool) -> Self {
        let binary = ty.iter().map(|(k, t)| (*k, if t == "Water" { t.clone() } else { "Land".to_string() })).collect();
        let lands = ty.iter().filter(|(_, t)| *t != "Water").map(|(k, t)| (*k, t.clone())).collect();
        ArtClassifier { water: ArtModel::train(rgb, w, h, &binary, train_on), land: ArtModel::train(rgb, w, h, &lands, train_on) }
    }

    /// Label of pixel `i` among the `present` types; `mask` (FO2 walk mask) decides water vs land
    /// where water is present. (A mod installed over Fallout 2 reads FO2's masks for tiles it has none
    /// of: those describe FO2's coast, not the mod's art.)
    fn label(&self, i: usize, present: &[&str], mask: Option<bool>) -> Option<String> {
        let lands: Vec<&str> = present.iter().copied().filter(|t| *t != "Water").collect();
        let has_water = present.contains(&"Water");
        let water = match (mask, has_water, lands.is_empty()) {
            (_, false, _) => false,
            (Some(v), true, _) => v,
            (None, true, true) => true,
            (None, true, false) => {
                let m = self.water.as_ref()?;
                let allowed = [m.class_index("Water")?, m.class_index("Land")?];
                m.predict(i, &allowed) == Some(allowed[0])
            }
        };
        if water { return Some("Water".to_string()); }
        match lands.as_slice() {
            [] => None,
            [only] => Some(only.to_string()),
            _ => {
                let m = self.land.as_ref()?;
                let allowed: Vec<usize> = lands.iter().filter_map(|t| m.class_index(t)).collect();
                m.predict(i, &allowed).map(|c| m.classes[c].clone())
            }
        }
    }
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

/// Saturated glowing red (the Glow crater's radiation veins); nothing else in FO1/FO2 art has it.
fn is_radiation_glow(p: [u8; 3]) -> bool {
    let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
    r >= 50.0 && r > 2.5 * g && r > 2.5 * b
}

/// Splits each square into SUB_PX blocks:
/// - Squares whose neighbourhood has one type keep it, so the art never invents terrain.
/// - Boundary squares take, per block, the art classifier's majority vote among the types present
///   in their 3x3 neighbourhood; FO2's walk mask decides water vs land where it exists.
/// - Blocks within reach of the Glow's radiation glow become Radiation.
fn refine_with_art(squares: &[RawTile], w_sq: usize, h_sq: usize, rgb: &[[u8; 3]], mask: &[Option<bool>]) -> Vec<RawTile> {
    let (w, h) = (w_sq * SQUARE_PX, h_sq * SQUARE_PX);
    let ty: HashMap<(i32, i32), String> = squares.iter().map(|t| ((t.x, t.y), t.tile_type.clone())).collect();
    let art = ArtClassifier::train(rgb, w, h, &ty, &|_, _| true);
    let glow: Vec<f64> = rgb.iter().map(|p| is_radiation_glow(*p) as u8 as f64).collect();
    let near_glow = crate::raster::box_mean(&glow, w, h, 15);

    let k = SQUARE_PX / SUB_PX;
    let mut out = Vec::with_capacity(squares.len() * k * k);
    for sq in squares {
        let (sx, sy) = (sq.x, sq.y);
        let mut present: Vec<&str> = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (sx + dx, sy + dy)))
            .filter_map(|p| ty.get(&p).map(|s| s.as_str())).collect();
        present.sort();
        present.dedup();
        for by in 0..k { for bx in 0..k {
            let mut tile_type = sq.tile_type.clone();
            let pixels = (0..SUB_PX).flat_map(|y| (0..SUB_PX).map(move |x| (sy as usize * SQUARE_PX + by * SUB_PX + y) * w + sx as usize * SQUARE_PX + bx * SUB_PX + x));
            if present.len() > 1 {
                let mut votes: HashMap<String, usize> = HashMap::new();
                for i in pixels.clone() {
                    if let Some(l) = art.label(i, &present, mask[i]) { *votes.entry(l).or_default() += 1; }
                }
                if let Some((t, _)) = votes.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))) { tile_type = t; }
            }
            // Glow pixels must be dense nearby (>1% of a 31x31 window), so stray red specks don't count.
            if pixels.clone().any(|i| near_glow[i] > 0.01) { tile_type = "Radiation".to_string(); }
            out.push(RawTile {
                x: sx * k as i32 + bx as i32, y: sy * k as i32 + by as i32,
                tile_type,
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

    #[test]
    fn msg_entries() {
        let msg = "# comment\n{1500}{}{Vault City}\n{1501}{voice}{Wind of\nWar}\n{1502}{}{}";
        let m = parse_msg(msg);
        assert_eq!(m.get(&1500).map(String::as_str), Some("Vault City"));
        assert_eq!(m.get(&1501).map(String::as_str), Some("Wind of\nWar"));
        assert_eq!(m.get(&1502).map(String::as_str), Some(""));
    }

    fn art(v: GameVariant) -> Option<(Vec<RawTile>, usize, usize, Vec<[u8; 3]>, Vec<Option<bool>>)> {
        let ex = FalloutInterplayExtractor::new(v);
        let dir = dat_extractor::find_steam_app(v.app_id(), v.name()).ok();
        let wm = ex.load_text_file(&dir, "WORLDMAP.TXT").ok()?;
        let (squares, ws, hs, _) = ex.parse_worldmap_txt(&wm).ok()?;
        match ex.load_world_art(&dir, ws as usize, hs as usize) {
            Ok((rgb, mask)) => Some((squares, ws as usize * SQUARE_PX, hs as usize * SQUARE_PX, rgb, mask)),
            Err(e) => { eprintln!("skipping: {:?} art unavailable ({})", v, e); None }
        }
    }

    /// The learned classifier's water must agree with FO2's exact walk masks on coastal pixels,
    /// since FO1 (which has no masks) relies on it alone.
    #[test]
    fn learned_sea_matches_fo2_masks() {
        let Some((squares, w, h, rgb, mask)) = art(GameVariant::Fallout2) else { return };
        let ty: HashMap<(i32, i32), String> = squares.iter().map(|t| ((t.x, t.y), t.tile_type.clone())).collect();
        let binary = ty.iter().map(|(k, t)| (*k, if t == "Water" { t.clone() } else { "Land".into() })).collect();
        let m = ArtModel::train(&rgb, w, h, &binary, &|_, _| true).expect("classifier");
        let all: Vec<usize> = (0..m.classes.len()).collect();
        let water = m.class_index("Water").unwrap();
        let (mut agree, mut total) = (0usize, 0usize);
        for y in 0..h { for x in 0..w {
            let (sx, sy) = ((x / SQUARE_PX) as i32, (y / SQUARE_PX) as i32);
            let coastal = (-1..=1).any(|dy| (-1..=1).any(|dx| ty.get(&(sx + dx, sy + dy)).map(|s| s.as_str()) == Some("Water")));
            if let (true, Some(mk)) = (coastal, mask[y * w + x]) {
                total += 1;
                agree += (mk == (m.predict(y * w + x, &all) == Some(water))) as usize;
            }
        }}
        let acc = agree as f64 / total as f64;
        println!("learned vs mask on {} coastal pixels: {:.3}", total, acc);
        assert!(acc > 0.9, "classifier accuracy {:.3}", acc);
    }

    /// Trained on half the squares (checkerboard), the classifier must recover the other half's
    /// interior squares. Production only ever chooses among the types around a square, so this
    /// measures the same decision: each held-out pixel against its own type and one other.
    #[test]
    fn classifier_recovers_held_out_square_types() {
        for v in [GameVariant::Fallout1, GameVariant::Fallout2] {
            let Some((squares, w, h, rgb, _)) = art(v) else { continue };
            let ty: HashMap<(i32, i32), String> = squares.iter().map(|t| ((t.x, t.y), t.tile_type.clone())).collect();
            let art = ArtClassifier::train(&rgb, w, h, &ty, &|x, y| (x + y) % 2 == 0);
            let mut types: Vec<&str> = ty.values().map(|s| s.as_str()).collect();
            types.sort();
            types.dedup();
            let mut per_type: HashMap<&str, (usize, usize)> = HashMap::new();
            for (&(sx, sy), t) in &ty {
                let interior = (-1..=1).all(|dy| (-1..=1).all(|dx| ty.get(&(sx + dx, sy + dy)).map_or(true, |v| v == t)));
                if (sx + sy) % 2 == 0 || !interior { continue; }
                for y in (0..SQUARE_PX).step_by(5) { for x in (0..SQUARE_PX).step_by(5) {
                    let i = (sy as usize * SQUARE_PX + y) * w + sx as usize * SQUARE_PX + x;
                    for other in types.iter().filter(|o| **o != t.as_str()) {
                        let e = per_type.entry(t.as_str()).or_default();
                        e.1 += 1;
                        e.0 += (art.label(i, &[t.as_str(), other], None).as_deref() == Some(t.as_str())) as usize;
                    }
                }}
            }
            let (agree, total) = per_type.values().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            let acc = agree as f64 / total as f64;
            println!("{:?}: held-out pairwise accuracy {:.3}; per type {:?}", v, acc,
                per_type.iter().map(|(t, (a, n))| (*t, format!("{:.2}", *a as f64 / *n as f64))).collect::<Vec<_>>());
            assert!(acc >= 0.85, "{:?} accuracy {:.3}", v, acc);
        }
    }

    /// Radiation comes from the Glow crater's art: present in FO1, absent in FO2.
    #[test]
    fn radiation_only_at_the_glow() {
        for (v, expect) in [(GameVariant::Fallout1, true), (GameVariant::Fallout2, false)] {
            let Some((squares, w, h, rgb, mask)) = art(v) else { continue };
            let tiles = refine_with_art(&squares, w / SQUARE_PX, h / SQUARE_PX, &rgb, &mask);
            let rad: Vec<(i32, i32)> = tiles.iter().filter(|t| t.tile_type == "Radiation").map(|t| (t.x, t.y)).collect();
            println!("{:?}: {} radiation blocks", v, rad.len());
            assert_eq!(!rad.is_empty(), expect);
            // FO1: all near the Glow (px 1223,1272 -> block 122,127).
            assert!(rad.iter().all(|&(x, y)| (x - 122).abs() <= 12 && (y - 127).abs() <= 12), "{:?}", rad);
        }
    }
}

/// A Fallout .msg file's `{number}{audio}{text}` entries, by number.
fn parse_msg(content: &str) -> HashMap<u32, String> {
    let mut fields = content.split('{').skip(1).map(|f| f.split_once('}').map_or(f, |(field, _)| field));
    let mut out = HashMap::new();
    while let (Some(n), Some(_audio), Some(text)) = (fields.next(), fields.next(), fields.next()) {
        if let Ok(n) = n.trim().parse() { out.insert(n, text.trim().to_string()); }
    }
    out
}

/// The first directory under `root` (breadth-first, `root` included) holding a file named `name`
/// (case-insensitive).
fn find_dir_with(root: &Path, name: &str) -> Option<PathBuf> {
    let mut queue = vec![root.to_path_buf()];
    while !queue.is_empty() {
        let mut next = Vec::new();
        for dir in queue {
            let entries: Vec<PathBuf> = fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
            if entries.iter().any(|p| p.file_name().is_some_and(|f| f.eq_ignore_ascii_case(name))) { return Some(dir); }
            next.extend(entries.into_iter().filter(|p| p.is_dir()));
        }
        queue = next;
    }
    None
}
