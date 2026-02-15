mod tile_gen;
mod tiled_structs;
mod map_gen;
mod schema;
mod extractors;
mod fixer;
mod pipeline;
mod rubbersheeting;
mod dat_extractor;

use std::path::Path;
use std::fs;
use crate::extractors::Extractor;
use crate::extractors::fallout1::Fallout1Extractor;
use crate::extractors::fallout2::Fallout2Extractor;

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
    let extractors: Vec<Box<dyn Extractor>> = vec![
        Box::new(Fallout1Extractor),
        // Box::new(Fallout2Extractor), // Commented out - focusing on Fallout 1 first
    ];

    pipeline::run_pipeline(extractors, maps_dir);

    println!("Done! stored in output/");
    Ok(())
}
