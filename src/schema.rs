use serde::{Serialize, Deserialize};

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
    Other,
    Misc,
}

impl IconType {
    pub fn from_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();
        
        if name_lower.contains("vault") {
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
            IconType::Other => "Other",
            IconType::Misc => "Misc",
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

#[derive(Serialize, Deserialize, Debug, Clone)]
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
    pub reference_lat: Option<f32>, // Real-world lat for (0,0) index
    #[serde(default)]
    pub reference_lon: Option<f32>, // Real-world lon for (0,0) index
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terrain_data: Option<String>, // WORLDMAP.TXT content if available
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
}
