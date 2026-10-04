use crate::schema::{RawMapData, RawLocation, RawTile, IconType};
use crate::extractors::Extractor;
use std::io;
use std::path::Path;
use std::fs;
use serde::{Deserialize, Serialize};
use log::{info, warn, error};

/// Manual extractor that reads a single JSON file
pub struct ManualExtractor {
    file_path: String,
    id: String,
    miles_per_hex: f32,
}

impl ManualExtractor {
    pub fn new(file_path: String, miles_per_hex: f32) -> Self {
        let id = Path::new(&file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("manual_unnamed")
            .to_string();
        Self { file_path, id, miles_per_hex }
    }
}

impl Extractor for ManualExtractor {
    fn get_id(&self) -> &str {
        &self.id
    }

    fn get_source_paths(&self) -> Vec<String> {
        vec![self.file_path.clone()]
    }

    fn get_code_path(&self) -> &str {
        "src/extractors/manual.rs"
    }

    fn get_cluster_id(&self) -> String {
        "manual".to_string()
    }

    fn extract(&self) -> RawMapData {
        match self.extract_from_json() {
            Ok(data) => data,
            Err(e) => {
                error!("Error extracting manual data from {}: {}", self.file_path, e);
                RawMapData {
                    tiles: vec![],
                    locations: vec![],
                    width: 0,
                    height: 0,
                    coordinate_system: crate::schema::CoordinateSystem::Absolute,
                    rows: None,
                    has_grid: false,
                    coordinate_scale: 1.0,
            priority: crate::schema::tier::MAIN,
                    reference_lon: None,
                    warnings: vec![],
                    terrain_data: None,
                    world_bounds: None,
                    ..Default::default()
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ManualMapFile {
    #[serde(default)]
    coordinate_system: crate::schema::CoordinateSystem,
    #[serde(default)]
    markers: Vec<ManualMarker>,
    #[serde(default)]
    terrain: Option<TerrainBuilder>,
    #[serde(default)]
    rows: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ManualMarker {
    name: String,
    #[serde(rename = "type")]
    marker_type: String, 
    #[serde(default)]
    lat: Option<f32>,
    #[serde(default)]
    lon: Option<f32>,
    #[serde(default)]
    x: Option<f32>, // Grid X (used if coordinate_system is Relative)
    #[serde(default)]
    y: Option<f32>, // Grid Y (used if coordinate_system is Relative)
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    pub anchor: Option<String>,
    #[serde(default)]
    pub terrain_override: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
enum TerrainBuilder {
    #[serde(rename = "circle")]
    Circle {
        #[serde(default)]
        center_lat: Option<f32>,
        #[serde(default)]
        center_lon: Option<f32>,
        #[serde(default)]
        center: Option<String>, // Alternative: place name for geocoding
        radius_hexes: u32,
        terrain_type: String,
    },
    #[serde(rename = "grid")]
    Grid {
        #[serde(default)]
        top_left_lat: Option<f32>,
        #[serde(default)]
        top_left_lon: Option<f32>,
        #[serde(default)]
        top_left: Option<String>, // Alternative: place name for geocoding
        hex_size_degrees: f32,
        tiles: Vec<Vec<String>>, // 2D array of terrain types
    },
}

impl ManualExtractor {
    fn extract_from_json(&self) -> io::Result<RawMapData> {
        info!("Reading manual map file: {}", self.file_path);
        let content = fs::read_to_string(&self.file_path)?;
        let map_file: ManualMapFile = serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let mut all_locations = Vec::new();
        let mut all_tiles = Vec::new();
        let has_grid = map_file.rows.is_some() || matches!(map_file.terrain, Some(TerrainBuilder::Grid { .. }));

        // 1. Process markers
        for marker in map_file.markers {
            let icon_type = IconType::from_name(&marker.marker_type);
            
            // For Relative system, prioritize x/y grid coordinates
            if map_file.coordinate_system == crate::schema::CoordinateSystem::Relative {
                if let (Some(gx), Some(gy)) = (marker.x, marker.y) {
                    all_locations.push(RawLocation {
                        name: marker.name.clone(),
                        x: gx,
                        y: gy,
                        icon_type,
                        real_lat: None,
                        real_lon: None,
                        terrain_override: None,
                        anchor: marker.anchor.clone(),
                    });
                    continue;
                }
            }

            // Fallback: Get real-world coordinates and use them as x/y temporarily
            let (lat, lon) = if let (Some(lat), Some(lon)) = (marker.lat, marker.lon) {
                (lat, lon)
            } else if let Some(location) = &marker.location {
                match crate::geocoding::geocode(location) {
                    Ok(coords) => coords,
                    Err(e) => {
                        warn!("Failed to geocode '{}': {}", location, e);
                        continue;
                    }
                }
            } else if let (Some(x), Some(y)) = (marker.x, marker.y) {
                 // Even in Absolute system, if ONLY x/y provided, we'll treat them as lat/lon? 
                 // No, let's treat them as raw game units.
                 (y, x) 
            } else {
                warn!("Marker '{}' in {} has insufficient data", marker.name, self.file_path);
                continue;
            };
            
            all_locations.push(RawLocation {
                name: marker.name,
                x: lon,
                y: lat,
                icon_type,
                real_lat: Some(lat),
                real_lon: Some(lon),
                terrain_override: None,
                anchor: marker.anchor.clone(),
            });
        }

        // 2. Process legacy terrain builder
        if let Some(terrain) = map_file.terrain {
            all_tiles.extend(self.generate_terrain(&terrain, &mut all_locations));
        }

        // 3. Process new 2D grid 'rows'
        if let Some(rows) = map_file.rows {
            for (row_idx, row) in rows.iter().enumerate() {
                for (col_idx, tile_type) in row.iter().enumerate() {
                    let mut final_type = tile_type.clone();
                    
                    // Handle Shorthand
                    if final_type.len() == 1 {
                        final_type = match final_type.to_lowercase().as_str() {
                            "g" => "Grass".to_string(),
                            "s" => "Sand".to_string(),
                            "w" => "Water".to_string(),
                            "m" => "Mountain".to_string(),
                            "r" => "Rock".to_string(),
                            "c" => "City".to_string(),
                            "u" | "." | " " => "unknown".to_string(),
                            _ => final_type,
                        };
                    }

                    let tt_lower = final_type.to_lowercase();
                    if tt_lower != "unknown" && tt_lower != "empty" && !tt_lower.is_empty() {
                        all_tiles.push(RawTile {
                            x: col_idx as i32,
                            y: row_idx as i32,
                            tile_type: final_type,
                            real_lat: None,
                            real_lon: None,
                            weight: 1.0,
                            anchor: None,
                            lod: false,
                        });
                    }
                }
            }
        }

        let width = all_tiles.iter().map(|t| t.x).max().unwrap_or(0) as u32 + 1;
        let height = all_tiles.iter().map(|t| t.y).max().unwrap_or(0) as u32 + 1;

        Ok(RawMapData {
            tiles: all_tiles,
            locations: all_locations,
            width,
            height,
            coordinate_system: map_file.coordinate_system,
            rows: None, // We've flattened it
            has_grid,
            coordinate_scale: 1.0,
            reference_lon: None,
            warnings: vec![],
            terrain_data: None,
            world_bounds: None,
            miles_per_unit: Some(self.miles_per_hex),
            priority: crate::schema::tier::MAIN,
            ..Default::default()
        })
    }

    fn generate_terrain(&self, builder: &TerrainBuilder, _all_locations: &mut Vec<RawLocation>) -> Vec<RawTile> {
        match builder {
            TerrainBuilder::Circle { center_lat, center_lon, center, radius_hexes, terrain_type } => {
                let (lat, lon) = if let (Some(la), Some(lo)) = (center_lat, center_lon) {
                    (*la, *lo)
                } else if let Some(q) = center {
                    match crate::geocoding::geocode(q) {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Failed to geocode terrain center '{}': {}", q, e);
                            return vec![];
                        }
                    }
                } else {
                    return vec![];
                };

                // Sample at half the hex spacing: a lattice as coarse as the hex grid leaves cells with
                // no sample point (hex rows are only sqrt(3)/2 of a hex apart), i.e. holes.
                let step_miles = self.miles_per_hex / 2.0;
                let hex_step_deg_lat = step_miles / 69.0;
                let hex_step_deg_lon = step_miles / (69.0 * lat.to_radians().cos());
                let resolution_scale = 1.0 / self.miles_per_hex;
                let effective_radius = (*radius_hexes as f32 * resolution_scale).round() as u32 * 2;

                self.generate_circle_terrain(lat, lon, hex_step_deg_lat, hex_step_deg_lon, effective_radius, terrain_type)
            }
            TerrainBuilder::Grid { top_left_lat, top_left_lon, top_left, hex_size_degrees: _, tiles } => {
                let (lat, lon) = if let (Some(la), Some(lo)) = (top_left_lat, top_left_lon) {
                    (*la, *lo)
                } else if let Some(q) = top_left {
                    match crate::geocoding::geocode(q) {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Failed to geocode terrain top_left '{}': {}", q, e);
                            return vec![];
                        }
                    }
                } else {
                    return vec![];
                };

                let hex_step_deg_lat = self.miles_per_hex / 69.0;
                let hex_step_deg_lon = self.miles_per_hex / (69.0 * lat.to_radians().cos());
                self.generate_grid_terrain(lat, lon, hex_step_deg_lat, hex_step_deg_lon, tiles)
            }
        }
    }

    fn generate_circle_terrain(&self, center_lat: f32, center_lon: f32, step_lat: f32, step_lon: f32, radius: u32, terrain_type: &str) -> Vec<RawTile> {
        let mut tiles = Vec::new();
        let radius_i = radius as i32;
        for dy in -radius_i..=radius_i {
            for dx in -radius_i..=radius_i {
                let dist_sq = (dx * dx + dy * dy) as f32;
                if dist_sq <= (radius as f32 * radius as f32) {
                    tiles.push(RawTile {
                        x: dx,
                        y: dy,
                        tile_type: terrain_type.to_string(),
                        real_lat: Some(center_lat + dy as f32 * step_lat),
                        real_lon: Some(center_lon + dx as f32 * step_lon),
                        weight: 1.0,
                        anchor: None,
                        lod: false,
                    });
                }
            }
        }
        tiles
    }

    fn generate_grid_terrain(&self, start_lat: f32, start_lon: f32, step_lat: f32, step_lon: f32, grid: &[Vec<String>]) -> Vec<RawTile> {
        let mut tiles = Vec::new();
        for (row_idx, row) in grid.iter().enumerate() {
            for (col_idx, terrain_type) in row.iter().enumerate() {
                let mut final_type = terrain_type.clone();
                
                // Handle Shorthand
                if final_type.len() == 1 {
                    final_type = match final_type.to_lowercase().as_str() {
                        "g" => "Grass".to_string(),
                        "s" => "Sand".to_string(),
                        "w" => "Water".to_string(),
                        "m" => "Mountain".to_string(),
                        "r" => "Rock".to_string(),
                        "c" => "City".to_string(),
                        "u" | "." | " " => "unknown".to_string(),
                        _ => final_type,
                    };
                }

                let tt_lower = final_type.to_lowercase();
                if tt_lower != "unknown" && tt_lower != "empty" && !tt_lower.is_empty() {
                    tiles.push(RawTile {
                        x: col_idx as i32,
                        y: row_idx as i32,
                        tile_type: final_type,
                        real_lat: Some(start_lat - row_idx as f32 * step_lat),
                        real_lon: Some(start_lon + col_idx as f32 * step_lon),
                        weight: 1.0,
                        anchor: None,
                        lod: false,
                    });
                }
            }
        }
        tiles
    }
}
