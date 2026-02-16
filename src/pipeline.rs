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

pub fn compute_source_hash(path: &str) -> String {
    let path = Path::new(path);
    if !path.exists() { return "dummy_hash".to_string(); }

    let mut hasher = Sha256::new();
    
    if path.is_dir() {
        hash_dir_recursive(path, &mut hasher);
    } else {
        let mut file = fs::File::open(path).expect("Failed to open source file for hashing");
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).expect("Failed to read source file");
        hasher.update(&buffer);
    }
    hex::encode(hasher.finalize())
}

fn hash_dir_recursive(dir: &Path, hasher: &mut Sha256) {
    let mut entries = fs::read_dir(dir).unwrap()
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>().unwrap();
    entries.sort(); // Consistent order

    for path in entries {
        if path.is_dir() {
            hash_dir_recursive(&path, hasher);
        } else {
            let mut file = fs::File::open(&path).unwrap();
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            hasher.update(path.to_string_lossy().as_bytes());
            hasher.update(&buffer);
        }
    }
}

pub fn run_pipeline(extractors: Vec<Box<dyn Extractor>>, output_dir: &Path) {
    let intermediate_dir = output_dir.join("intermediate");
    fs::create_dir_all(&intermediate_dir).unwrap();

    let mut raw_maps = Vec::new();
    let mut extractor_hashes = Vec::new();

    // --- STAGE 1: EXTRACTORS ---
    for extractor in &extractors {
        let id = extractor.get_id();
        let source_file = format!("src/extractors/{}.rs", id); // Convention: ID matches filename
        
        let mut hasher = Sha256::new();
        // 1. Hash the extractor code
        hasher.update(compute_source_hash(&source_file).as_bytes());
        
        // 2. Hash any additional source paths (e.g. manual JSON directory)
        for path in extractor.get_source_paths() {
            hasher.update(compute_source_hash(&path).as_bytes());
        }
        
        let current_hash = hex::encode(hasher.finalize());
        
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
         // Generate debug raw view for EACH map
         let gid_map = map_gen::generate_tsj(output_dir).unwrap(); 
         
         for (i, raw_data) in raw_maps.iter().enumerate() {
             let id = if i < extractors.len() { extractors[i].get_id() } else { "unknown" };
             let debug_filename = format!("raw_extracted_{}.tmj", id);
             let debug_path = output_dir.join(&debug_filename);
             
             // We need to modify generate_raw_tmj to accept a custom filename or rename it after
             // Currently generate_raw_tmj hardcodes "raw_extracted.tmj"
             // Let's modify map_gen::generate_raw_tmj signature to take a filename? 
             // Or simpler: change run_pipeline to copy the file.
             
             if let Err(e) = map_gen::generate_raw_tmj(output_dir, raw_data, &gid_map) {
                 println!("Warning: Failed to generate raw debug map for {}: {}", id, e);
             } else {
                 // Rename the default output
                 let default_out = output_dir.join("raw_extracted.tmj");
                 if default_out.exists() {
                     fs::rename(&default_out, &debug_path).unwrap();
                     println!("Generated debug map: {}", debug_filename);
                 }
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

    // --- STAGE 3.5: MARKER EXPORT ---
     let marker_file = output_dir.join("markers.json");
     if let Ok(json) = serde_json::to_string_pretty(&final_fixed_data.locations) {
         fs::write(&marker_file, json).expect("Failed to write markers.json");
         println!("[Pipeline] Exported {} markers to markers.json", final_fixed_data.locations.len());
     }

    // --- STAGE 4: RASTERIZER ---
    let output_png = output_dir.join("rendered_map.png");

    println!("[Rasterizer] Running...");
    
    // 1. Render Main Map
    let status = Command::new("tmxrasterizer")
        .arg(&map_file)
        .arg(&output_png)
        .status();

    match status {
        Ok(s) if s.success() => println!("[Rasterizer] Success: {:?}", output_png),
        Ok(s) => eprintln!("[Rasterizer] Failed with exit code: {:?}", s.code()),
        Err(e) => eprintln!("[Rasterizer] Failed to execute tmxrasterizer: {}", e),
    }

    // 2. Render All Raw Debug Maps
    // Looking for raw_extracted_*.tmj
    if let Ok(entries) = fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("raw_extracted_") && name.ends_with(".tmj") {
                    let png_path = path.with_extension("png");
                     println!("[Rasterizer] Rendering debug map: {}", name);
                     let _ = Command::new("tmxrasterizer")
                        .arg(&path)
                        .arg(&png_path)
                        .status();
                }
            }
        }
    }

}
