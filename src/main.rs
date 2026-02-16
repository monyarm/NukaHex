mod tile_gen;
mod tiled_structs;
mod map_gen;
mod schema;
mod extractors;
mod fixer;
mod pipeline;
mod rubbersheeting;
mod dat_extractor;
mod geocoding;

use std::path::Path;
use std::fs;
use crate::extractors::Extractor;
use crate::extractors::interplay::{FalloutInterplayExtractor, GameVariant};

fn main() -> std::io::Result<()> {
    let tiles_dir = Path::new("output/tiles");
    let maps_dir = Path::new("output/maps");

    if !tiles_dir.exists() {
        fs::create_dir_all(tiles_dir)?;
    }
    if !maps_dir.exists() {
        fs::create_dir_all(maps_dir)?;
    }

    println!("--- Phase 1: Asset Generation ---");
    tile_gen::generate_tiles(tiles_dir)?;

    println!("--- Phase 2: Pipeline Execution ---");
    // Run both Fallout 1 and Fallout 2 extractors
    let extractors: Vec<Box<dyn Extractor>> = vec![
        Box::new(FalloutInterplayExtractor::new(GameVariant::Fallout1)),
        Box::new(FalloutInterplayExtractor::new(GameVariant::Fallout2)),
        Box::new(extractors::manual::ManualExtractor::new("input/manual".to_string())),
    ];

    pipeline::run_pipeline(extractors, maps_dir);

    println!("Done! stored in output/");
    Ok(())
}
