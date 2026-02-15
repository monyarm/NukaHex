use std::fs;
use std::path::Path;
use sha2::{Sha256, Digest};
use std::io::Read;
use serde_json::Value;
use std::process::Command;

use crate::schema::{RawMapData, FixedMapData};
use crate::extractors::Extractor;
use crate::fixer;
use crate::map_gen; // Reusing map_gen as Builder for now, will refactor later

pub fn compute_source_hash(file_path: &str) -> String {
    let mut file = fs::File::open(file_path).expect("Failed to open source file for hashing");
    let mut hasher = Sha256::new();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Failed to read source file");
    hasher.update(&buffer);
    hex::encode(hasher.finalize())
}

pub fn run_pipeline(extractors: Vec<Box<dyn Extractor>>, output_dir: &Path) {
    let intermediate_dir = output_dir.join("intermediate");
    fs::create_dir_all(&intermediate_dir).unwrap();

    let mut raw_maps = Vec::new();
    let mut extractor_hashes = Vec::new();

    // --- STAGE 1: EXTRACTORS ---
    for extractor in extractors {
        let id = extractor.get_id();
        let source_file = format!("src/extractors/{}.rs", id); // Convention: ID matches filename
        
        let current_hash = if Path::new(&source_file).exists() {
            compute_source_hash(&source_file)
        } else {
            "dummy_hash".to_string() 
        };
        
        // Check Cache
        let cache_file = intermediate_dir.join(format!("{}.json", id));
        let hash_file = intermediate_dir.join(format!("{}.hash", id));
        
        let mut cached = false;
        if cache_file.exists() && hash_file.exists() {
            let saved_hash = fs::read_to_string(&hash_file).unwrap();
            if saved_hash == current_hash {
                println!("[Extractor: {}] Using Cache.", id);
                let data: RawMapData = serde_json::from_str(&fs::read_to_string(&cache_file).unwrap()).unwrap();
                raw_maps.push(data);
                cached = true;
            }
        }

        if !cached {
            println!("[Extractor: {}] Running...", id);
            let data = extractor.extract();
            
            // Save Cache
            fs::write(&cache_file, serde_json::to_string_pretty(&data).unwrap()).unwrap();
            fs::write(&hash_file, &current_hash).unwrap();
            
            raw_maps.push(data);
        }
        extractor_hashes.push(current_hash);
    }

    // --- STAGE 2: FIXER ---
    let fixer_source = "src/fixer.rs";
    let fixer_code_hash = compute_source_hash(fixer_source);
    // Combined hash of code + all inputs
    let mut hasher = Sha256::new();
    hasher.update(fixer_code_hash.as_bytes());
    for h in &extractor_hashes {
        hasher.update(h.as_bytes());
    }
    let combined_fixer_hash = hex::encode(hasher.finalize());
    
    let fixed_cache = intermediate_dir.join("fixed_data.json");
    let fixed_hash_file = intermediate_dir.join("fixed_data.hash");
    
    let mut fixed_data: Option<FixedMapData> = None;

    if fixed_cache.exists() && fixed_hash_file.exists() {
        let saved_hash = fs::read_to_string(&fixed_hash_file).unwrap();
        if saved_hash == combined_fixer_hash {
            println!("[Fixer] Using Cache.");
            fixed_data = Some(serde_json::from_str(&fs::read_to_string(&fixed_cache).unwrap()).unwrap());
        }
    }

    let final_fixed_data = if let Some(data) = fixed_data {
        data
    } else {
         println!("[Fixer] Running...");
         // Check if we have fallout1 data and generate raw debug map
         if let Some(fallout1_data) = raw_maps.first() { 
             // Assuming first map is fallout1 for now, or check metadata if available
             // Generate debug raw view
             let gid_map = map_gen::generate_tsj(output_dir).unwrap(); 
             if let Err(e) = map_gen::generate_raw_tmj(output_dir, fallout1_data, &gid_map) {
                 println!("Warning: Failed to generate raw debug map: {}", e);
             }
         }

         let data = fixer::fix(raw_maps);
         
         fs::write(&fixed_cache, serde_json::to_string_pretty(&data).unwrap()).unwrap();
         fs::write(&fixed_hash_file, &combined_fixer_hash).unwrap();
         data
    };

    // --- STAGE 3: BUILDER ---
    // For now, we'll adapt our old map_gen to take FixedMapData later.
    // Hash builder code + fixed data input
    let builder_source = "src/map_gen.rs";
    let builder_code_hash = compute_source_hash(builder_source);
    let mut b_hasher = Sha256::new();
    b_hasher.update(builder_code_hash.as_bytes());
    b_hasher.update(combined_fixer_hash.as_bytes()); // Indirectly hashes input data
    let builder_hash = hex::encode(b_hasher.finalize());
    
    let builder_hash_file = intermediate_dir.join("builder.hash");
    let map_file = output_dir.join("map.tmj");

    let run_builder = if builder_hash_file.exists() && map_file.exists() {
         let saved = fs::read_to_string(&builder_hash_file).unwrap();
         if saved == builder_hash {
             println!("[Builder] Up to date (Skipping).");
             false
         } else { true }
    } else { true };

    if run_builder {
        println!("[Builder] Running...");
        // Call map generator with FixedMapData
        let gid_map = map_gen::generate_tsj(output_dir).unwrap();
        map_gen::generate_tmj(output_dir, &final_fixed_data, &gid_map).unwrap();
        
        fs::write(&builder_hash_file, &builder_hash).unwrap();
    }

    // --- STAGE 4: RASTERIZER ---
    let output_png = output_dir.join("rendered_map.png");

    // Run if builder ran OR if output file doesn't exist
    if run_builder || !output_png.exists() {
        println!("[Rasterizer] Running...");
        let status = Command::new("tmxrasterizer")
            .arg(&map_file)
            .arg(&output_png)
            .status();

        match status {
            Ok(s) if s.success() => println!("[Rasterizer] Success: {:?}", output_png),
            Ok(s) => eprintln!("[Rasterizer] Failed with exit code: {:?}", s.code()),
            Err(e) => eprintln!("[Rasterizer] Failed to execute tmxrasterizer: {}", e),
        }

        // Also rasterize raw debug map if it exists
        let raw_map_file = output_dir.join("raw_extracted.tmj");
        let raw_png = output_dir.join("raw_extracted.png");
        if raw_map_file.exists() {
             let status = Command::new("tmxrasterizer")
                .arg(&raw_map_file)
                .arg(&raw_png)
                .status();
             match status {
                Ok(s) if s.success() => println!("[Rasterizer] Success: {:?}", raw_png),
                _ => eprintln!("[Rasterizer] Failed to rasterize raw map (tmxrasterizer might be missing or failed)"),
             }
        }
    } else {
        println!("[Rasterizer] Up to date (Skipping).");
    }
}
