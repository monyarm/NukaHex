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
mod caching;
mod clustering; // Added clustering module
mod utils;

use std::path::Path;
use std::fs;
use crate::extractors::Extractor;
use crate::extractors::interplay::{FalloutInterplayExtractor, GameVariant};

use simplelog::*;
use log::info;
use std::fs::File;

fn main() -> std::io::Result<()> {
    // Initialize logging
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .build();

    // Create output directory first
    fs::create_dir_all("output")?;

    let log_file = File::create("output/fallout_map.log")?;

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Info, config.clone(), TerminalMode::Mixed, ColorChoice::Auto),
            WriteLogger::new(LevelFilter::Info, config, log_file),
        ]
    ).unwrap();

    let tiles_dir = Path::new("output/tiles");
    let maps_dir = Path::new("output/maps");

    if !tiles_dir.exists() {
        fs::create_dir_all(tiles_dir)?;
    }
    if !maps_dir.exists() {
        fs::create_dir_all(maps_dir)?;
    }

    info!("--- Phase 1: Asset Generation ---");
    tile_gen::generate_tiles(tiles_dir)?;

    info!("--- Phase 2: Pipeline Execution ---");
    let mut extractors: Vec<Box<dyn Extractor>> = vec![
        Box::new(FalloutInterplayExtractor::new(GameVariant::Fallout1)),
        Box::new(FalloutInterplayExtractor::new(GameVariant::Fallout2)),
        Box::new(FalloutInterplayExtractor::new(GameVariant::FalloutBoS)),
    ];

    // Recursively find all JSON files in input/manual
    fn find_manual_files(dir: &Path, files: &mut Vec<String>) -> std::io::Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    find_manual_files(&path, files)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    files.push(path.to_string_lossy().to_string());
                }
            }
        }
        Ok(())
    }

    let mut manual_files = Vec::new();
    let _ = find_manual_files(Path::new("input/manual"), &mut manual_files);
    
    for file in manual_files {
        extractors.push(Box::new(crate::extractors::manual::ManualExtractor::new(file, 6.0)));
    }

    pipeline::run_pipeline(extractors, maps_dir);

    info!("Done! stored in output/");
    Ok(())
}
