use crate::schema::{RawMapData, RawLocation, RawTile, IconType};
use crate::extractors::Extractor;
use std::io;
use std::path::Path;
use std::fs;
use serde::{Deserialize, Serialize};

/// Manual extractor that reads JSON files from inputs/ directory
pub struct ManualExtractor {
    input_dir: String,
    miles_per_hex: f32,
}

impl ManualExtractor {
    pub fn new(input_dir: String, miles_per_hex: f32) -> Self {
        Self { input_dir, miles_per_hex }
    }
}

impl Extractor for ManualExtractor {
    fn get_id(&self) -> &str {
        "manual"
    }

    fn get_source_paths(&self) -> Vec<String> {
        vec![self.input_dir.clone()]
    }

    fn extract(&self) -> RawMapData {
        match self.extract_from_json() {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Error extracting manual data: {}", e);
                eprintln!("Returning empty data");
                RawMapData {
                    tiles: vec![],
                    locations: vec![],
                    width: 0,
                    height: 0,
                    coordinate_scale: 1.0,
                    reference_lat: None,
                    reference_lon: None,
                    terrain_data: None,
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ManualMapFile {
    markers: Vec<ManualMarker>,
    #[serde(default)]
    terrain: Option<TerrainBuilder>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ManualMarker {
    name: String,
    #[serde(rename = "type")]
    marker_type: String, // "vault", "city", etc.
    #[serde(default)]
    lat: Option<f32>,
    #[serde(default)]
    lon: Option<f32>,
    #[serde(default)]
    location: Option<String>, // Alternative: place name/address for geocoding
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
        println!("Extracting Manual Data from {}...", self.input_dir);
        
        let input_path = Path::new(&self.input_dir);
        if !input_path.exists() {
            println!("Input directory does not exist: {}", self.input_dir);
            return Ok(RawMapData {
                tiles: vec![],
                locations: vec![],
                width: 0,
                height: 0,
                coordinate_scale: 1.0,
                reference_lat: None,
                reference_lon: None,
                terrain_data: None,
            });
        }

        let mut all_locations = Vec::new();
        let mut all_tiles = Vec::new();

        // Read all .json files recursively
        self.load_json_files_recursive(input_path, &mut all_locations, &mut all_tiles)?;

        println!("Extracted {} manual markers and {} terrain tiles", 
                 all_locations.len(), all_tiles.len());

        // Calculate bounds
        let max_x = all_tiles.iter().map(|t| t.x).max().unwrap_or(0);
        let max_y = all_tiles.iter().map(|t| t.y).max().unwrap_or(0);

        Ok(RawMapData {
            tiles: all_tiles,
            locations: all_locations,
            width: (max_x + 1) as u32,
            height: (max_y + 1) as u32,
            coordinate_scale: 1.0,
            reference_lat: None,
            reference_lon: None,
            terrain_data: None,
        })
    }

    fn load_json_files_recursive(
        &self,
        dir: &Path,
        all_locations: &mut Vec<RawLocation>,
        all_tiles: &mut Vec<RawTile>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                // Recurse into subdirectories
                self.load_json_files_recursive(&path, all_locations, all_tiles)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                println!("Reading manual map file: {:?}", path);
                let content = fs::read_to_string(&path)?;
                
                match serde_json::from_str::<ManualMapFile>(&content) {
                    Ok(map_file) => {
                        // Process markers
                        for marker in map_file.markers {
                            let icon_type = IconType::from_name(&marker.marker_type);
                            
                            // Get coordinates either from lat/lon or by geocoding location
                            let (lat, lon) = if let (Some(lat), Some(lon)) = (marker.lat, marker.lon) {
                                (lat, lon)
                            } else if let Some(location) = &marker.location {
                                match crate::geocoding::geocode(location) {
                                    Ok(coords) => coords,
                                    Err(e) => {
                                        eprintln!("Failed to geocode '{}': {}", location, e);
                                        continue;
                                    }
                                }
                            } else {
                                eprintln!("Marker '{}' has neither lat/lon nor location", marker.name);
                                continue;
                            };
                            
                            // Store lat/lon as x/y (will be handled by fixer)
                            all_locations.push(RawLocation {
                                name: marker.name,
                                x: lon,
                                y: lat,
                                icon_type,
                                real_lat: Some(lat),
                                real_lon: Some(lon),
                            });
                        }

                        // Process terrain
                        if let Some(terrain) = map_file.terrain {
                            let generated_tiles = self.generate_terrain(&terrain, all_locations);
                            all_tiles.extend(generated_tiles);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to parse {:?}: {}", path, e);
                    }
                }
            }
        }
        Ok(())
    }

    fn generate_terrain(&self, builder: &TerrainBuilder, all_locations: &mut Vec<RawLocation>) -> Vec<RawTile> {
        match builder {
            TerrainBuilder::Circle { center_lat, center_lon, center, radius_hexes, terrain_type } => {
                // Resolve center point
                let (lat, lon) = if let (Some(la), Some(lo)) = (center_lat, center_lon) {
                    (*la, *lo)
                } else if let Some(q) = center {
                    match crate::geocoding::geocode(q) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("Failed to geocode terrain center '{}': {}", q, e);
                            return vec![];
                        }
                    }
                } else {
                    eprintln!("Circle terrain has no center");
                    return vec![];
                };

                let hex_step_deg_lat = self.miles_per_hex / 69.0;
                let hex_step_deg_lon = self.miles_per_hex / 56.5; 

                // Scale radius based on resolution (Input radius is in "Standard Hexes" i.e. 6 miles)
                // If miles_per_hex is 1.0, we need 6x the radius to cover the same distance.
                let resolution_scale = 6.0 / self.miles_per_hex;
                let effective_radius = (*radius_hexes as f32 * resolution_scale).round() as u32;

                self.generate_circle_terrain(lat, lon, hex_step_deg_lat, hex_step_deg_lon, effective_radius, terrain_type)
            }
            TerrainBuilder::Grid { top_left_lat, top_left_lon, top_left, hex_size_degrees, tiles } => {
                let (lat, lon) = if let (Some(la), Some(lo)) = (top_left_lat, top_left_lon) {
                    (*la, *lo)
                } else if let Some(q) = top_left {
                    match crate::geocoding::geocode(q) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("Failed to geocode terrain top_left '{}': {}", q, e);
                            return vec![];
                        }
                    }
                } else {
                    eprintln!("Grid terrain has no top_left");
                    return vec![];
                };

                let hex_step_deg_lat = self.miles_per_hex / 69.0;
                let hex_step_deg_lon = self.miles_per_hex / 56.5;

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
                if terrain_type != "empty" && !terrain_type.is_empty() {
                    tiles.push(RawTile {
                        x: col_idx as i32,
                        y: row_idx as i32,
                        tile_type: terrain_type.clone(),
                        real_lat: Some(start_lat - row_idx as f32 * step_lat), // Lat decreases southwards
                        real_lon: Some(start_lon + col_idx as f32 * step_lon),
                    });
                }
            }
        }
        tiles
    }
}
