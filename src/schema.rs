use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MapBounds {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum IconType {
    City,
    Vault,
    Store,
    Casino,
    Hospital,
    Radio,
    Monument,
    Settlement,
    Landmark, // major non-settlement site: base, facility, natural/man-made landmark
    Other,    // minor or random-encounter location
    Misc,
    Detail,   // inside a summarized DLC worldspace: on the full (`_all`) map only
}

impl IconType {
    pub fn from_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();
        
        if name_lower == "landmark" {
            IconType::Landmark
        } else if name_lower == "settlement" {
            IconType::Settlement
        } else if name_lower == "monument" {
            IconType::Monument
        } else if name_lower.contains("vault") {
            IconType::Vault
        } else if name_lower.contains("store") || name_lower.contains("shop") || name_lower.contains("market") {
            IconType::Store
        } else if name_lower.contains("casino") {
            IconType::Casino
        } else if name_lower.contains("hospital") || name_lower.contains("medical") || name_lower.contains("clinic") {
            IconType::Hospital
        } else if name_lower.contains("radio") || name_lower.contains("broadcast") {
            IconType::Radio
        } else if name_lower.contains("city") 
            || name_lower.contains("town") 
            || name_lower.contains("sands")
            || name_lower.contains("junktown")
            || name_lower.contains("hub")
            || name_lower.contains("necropolis")
            || name_lower.contains("boneyard")
            || name_lower.contains("cathedral") {
            IconType::City
        } else {
            IconType::Other
        }
    }
    
    pub fn to_string(&self) -> &'static str {
        match self {
            IconType::City => "City",
            IconType::Vault => "Vault",
            IconType::Store => "Store",
            IconType::Casino => "Casino",
            IconType::Hospital => "Hospital",
            IconType::Radio => "Radio", 
            IconType::Monument => "Monument",
            IconType::Settlement => "Settlement",
            IconType::Landmark => "Landmark",
            IconType::Other => "Other",
            IconType::Misc => "Misc",
            IconType::Detail => "Detail",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum CoordinateSystem {
    Absolute, // Uses real_lat/real_lon as primary, or rubbersheeting
    Relative, // Uses unknown hex-grid, solves based on anchors
}

impl Default for CoordinateSystem {
    fn default() -> Self { CoordinateSystem::Absolute }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RawMapData {
    pub locations: Vec<RawLocation>,
    pub tiles: Vec<RawTile>,

    pub width: u32,
    pub height: u32,

    #[serde(default)]
    pub coordinate_system: CoordinateSystem,

    #[serde(default)]
    pub rows: Option<Vec<Vec<String>>>, // 2D array input for terrain
    
    #[serde(default)]
    pub has_grid: bool, // Flag for debug map generation
    
    #[serde(default = "default_coordinate_scale")]
    pub coordinate_scale: f32, // How many game units per tile (Interplay=50.0, Manual=1.0)
    #[serde(default)]
    pub reference_lon: Option<f32>, // Real-world lon for (0,0) index
    #[serde(default)]
    pub warnings: Vec<String>, // Any extractor warnings generated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terrain_data: Option<String>, // WORLDMAP.TXT content if available
    #[serde(default)]
    pub world_bounds: Option<MapBounds>, // Official worldspace extents
    #[serde(default)]
    pub miles_per_unit: Option<f32>, // Relative maps: real miles per grid unit, if known
    /// Source priority tier (see `tier`); sets vote weight.
    #[serde(default)]
    pub priority: u8,
    /// The markers only anchor the warp (a mod placing terrain, not markers): they are not placed.
    #[serde(default)]
    pub anchor_only_markers: bool,
    /// Extractor-derived anchors (e.g. drawn state borders), in game coordinates.
    #[serde(default)]
    pub control_points: Vec<RawControlPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawControlPoint {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub lat: f32,
    pub lon: f32,
}

/// Source priority tiers, strongest first (see `priority_weight` for their vote weights).
pub mod tier {
    /// The main games (Fallout 1-4, New Vegas) and manual locations.
    pub const MAIN: u8 = 1;
    pub const FALLOUT_76: u8 = 2;
    /// Simulations of the past, set before the games (Operation Anchorage's Alaska, 2077).
    pub const PAST_SIMULATION: u8 = 3;
    /// Fallout Tactics, Van Buren.
    pub const SPIN_OFF: u8 = 4;
    pub const BROTHERHOOD_OF_STEEL: u8 = 5;
    pub const MODS: u8 = 6;
    pub const WEAKEST: u8 = MODS;
}

/// Vote weight of a source priority tier: each tier counts double the next, except the simulation of
/// the past, slightly under Fallout 76 (16/8/6/4/2/1 for tiers 1..6; 0, the default, counts as 1).
pub fn priority_weight(priority: u8) -> f32 {
    const WEIGHTS: [f32; tier::WEAKEST as usize] = [16.0, 8.0, 6.0, 4.0, 2.0, 1.0];
    WEIGHTS[(priority.clamp(tier::MAIN, tier::WEAKEST) - 1) as usize]
}

fn default_coordinate_scale() -> f32 {
    50.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawLocation {
    pub name: String,
    pub x: f32, // Raw game coordinates
    pub y: f32,
    pub icon_type: IconType,
    #[serde(default)]
    pub real_lat: Option<f32>,
    #[serde(default)]
    pub real_lon: Option<f32>,
    #[serde(default)]
    pub terrain_override: Option<String>,
    #[serde(default)]
    pub anchor: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RawTile {
    pub x: i32,
    pub y: i32,
    pub tile_type: String, // "Grass", "Water", "Sand"
    #[serde(default)]
    pub real_lat: Option<f32>,
    #[serde(default)]
    pub real_lon: Option<f32>,
    #[serde(default = "default_weight")]
    pub weight: f32,
    #[serde(default)]
    pub anchor: Option<String>,
    /// Land the game only shows from afar (its LOD terrain, beyond its own map): it votes under every
    /// source's own land.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lod: bool,
}

fn default_weight() -> f32 { 1.0 }

// FixedMapData might be structurally identical but semantically "correct" 
// (e.g. coordinates are now finalized map grid units)
// For now, we reuse similar structures or define a new one if transformations add new fields.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixedMapData {
    pub locations: Vec<FixedLocation>,
    pub tiles: Vec<FixedTile>,
    pub width: u32,
    pub height: u32,
    pub global_offset_x: i32,
    pub global_offset_y: i32,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub world_bounds: Option<MapBounds>,
    /// Places far from the main landmass drawn in a box off its coast (as US maps draw Alaska).
    #[serde(default)]
    pub insets: Vec<Inset>,
}

/// An inset: the grid box (inclusive) its hexes occupy, and the real region it shows.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Inset {
    pub name: String,
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixedLocation {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub icon_type: IconType,
    #[serde(default)]
    pub real_lat: Option<f32>,
    #[serde(default)]
    pub real_lon: Option<f32>,
    #[serde(default)]
    pub anchor: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixedTile {
    pub x: i32,
    pub y: i32,
    pub tile_type: String,
    #[serde(default)]
    pub real_lat: Option<f32>,
    #[serde(default)]
    pub real_lon: Option<f32>,
    #[serde(default)]
    pub anchor: Option<String>,
    /// Real-world region (index into `borders::REGIONS`) of a land tile, for drawing borders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<u16>,
    /// Only LOD land voted here (see `RawTile::lod`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lod: bool,
}
