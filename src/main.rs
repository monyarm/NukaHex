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
mod frm;

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
    let mut game_extractors: Vec<Box<dyn Extractor>> = vec![
        Box::new(FalloutInterplayExtractor::new(GameVariant::Fallout1)),
        Box::new(FalloutInterplayExtractor::new(GameVariant::Fallout2)),
    ];

    // To prevent long runs parsing huge 4/76 grids on every test, we usually control via CLI args.
    // For now we will add Fallout 4 to the default pipeline to test it since that's what we built!
    // `--no-fo4` skips the experimental Fallout 4 extractor.
    use crate::extractors::bethesda::{BethesdaExtractor, BethesdaGameVariant};
    if !std::env::args().any(|a| a == "--no-fo4") {
        game_extractors.push(Box::new(BethesdaExtractor::new(BethesdaGameVariant::Fallout4, vec!["Fallout4.esm".to_string()], "Commonwealth")));
    }

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
    
    let mut manual_extractors: Vec<Box<dyn Extractor>> = Vec::new();
    for file in manual_files {
        manual_extractors.push(Box::new(crate::extractors::manual::ManualExtractor::new(file, 6.0)));
    }

    let mut target_cluster = None;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--cluster" && i + 1 < args.len() {
            target_cluster = Some(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }

    if let Some(ref c) = target_cluster {
        info!("Running in single-cluster mode for: {}", c);
    }

    pipeline::run_pipeline(game_extractors, manual_extractors, maps_dir, target_cluster);

    info!("Done! stored in output/");
    Ok(())
}
