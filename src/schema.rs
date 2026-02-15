use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum IconType {
    City,
    Vault,
    Store,
    Casino,
    Misc,
}

impl IconType {
    pub fn from_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();
        
        if name_lower.contains("vault") {
            IconType::Vault
        } else if name_lower.contains("store") || name_lower.contains("shop") {
            IconType::Store
        } else if name_lower.contains("casino") {
            IconType::Casino
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
            IconType::Misc
        }
    }
    
    pub fn to_string(&self) -> &'static str {
        match self {
            IconType::City => "City",
            IconType::Vault => "Vault",
            IconType::Store => "Store",
            IconType::Casino => "Casino",
            IconType::Misc => "Misc",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RawMapData {
    pub locations: Vec<RawLocation>,
    pub tiles: Vec<RawTile>,
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terrain_data: Option<String>, // WORLDMAP.TXT content if available
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawLocation {
    pub name: String,
    pub x: f32, // Raw game coordinates
    pub y: f32,
    pub icon_type: IconType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RawTile {
    pub x: u32,
    pub y: u32,
    pub tile_type: String, // "Grass", "Water", "Sand"
}

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
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FixedTile {
    pub x: u32,
    pub y: u32,
    pub tile_type: String,
}
