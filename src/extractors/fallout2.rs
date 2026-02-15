use crate::schema::{RawMapData, RawLocation, RawTile};
use crate::extractors::Extractor;

pub struct Fallout2Extractor;

impl Extractor for Fallout2Extractor {
    fn get_id(&self) -> &str {
        "fallout2"
    }

    fn extract(&self) -> RawMapData {
        // Mock Data for Fallout 2 (Northern California / Oregon)
        // CHANGED: Trigger Cache Invalidation
        println!("Extracting Fallout 2 Data...");

        let width = 20;
        let height = 20;
        let mut tiles = Vec::new();

        // Simple rocky grid
        for y in 0..height {
            for x in 0..width {
                tiles.push(RawTile {
                    x: x + 25, // Artificial offset to place it "North" of FO1
                    y,
                    tile_type: "Rock".to_string(),
                });
            }
        }

        let locations = vec![
            RawLocation { name: "Arroyo".to_string(), x: 15.0, y: 15.0, icon_type: crate::schema::IconType::City },
            RawLocation { name: "Vault City".to_string(), x: 20.0, y: 18.0, icon_type: crate::schema::IconType::Vault },
        ];

        RawMapData {
            tiles,
            locations,
            width,
            height,
            terrain_data: None,
        }
    }
}
