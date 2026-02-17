use std::fs;
use std::path::Path;
use sha2::{Sha256, Digest};
use std::io::Read;

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
             if id == "manual" { continue; }
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

         let data = fixer::fix(raw_maps.clone(), None); // Default 6.0 miles/hex resolution
         
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
        
        // 1. Generate map with ALL markers
        map_gen::generate_tmj(output_dir, "map_all.tmj", &final_fixed_data, &gid_map, None, "generic.tsj").unwrap();

        // 2. Generate map with FILTERED markers (Cities, Vaults, Monuments)
        // Note: Filters are case-insensitive
        let allowed_types = vec!["City", "Vault", "Monument", "Settlement", "Ruins", "Landmark"]; 
        map_gen::generate_tmj(output_dir, "map.tmj", &final_fixed_data, &gid_map, Some(&allowed_types), "generic.tsj").unwrap();
        
        fs::write(&builder_hash_file, &builder_hash).unwrap();
    }

    // --- STAGE 3.1: CITY MAP GENERATION ---
    println!("[Pipeline] Generating City Maps...");
    let cities_dir = output_dir.join("cities");
    fs::create_dir_all(&cities_dir).unwrap();

    // 1. Identify Target Locations
    let city_targets: Vec<&crate::schema::FixedLocation> = final_fixed_data.locations.iter()
        .filter(|loc| {
            let t = loc.icon_type.to_string();
            matches!(t, "City" | "Vault" | "Monument" | "Settlement" | "Ruins" | "Landmark")
        })
        .collect();

    if !city_targets.is_empty() {
        println!("Found {} city/vault/landmark targets for detailed maps.", city_targets.len());

        // 2. Cluster Locations (Simple Distance-Based Clustering)
        // Groups locations within 30 miles (15 mile radius overlap)
        let mut clusters: Vec<Vec<&crate::schema::FixedLocation>> = Vec::new();
        
        for loc in &city_targets {
            let mut added_to_cluster = false;
            // Check existing clusters
            for cluster in &mut clusters {
                 // Check if ANY point in the cluster is close enough to ANY point in the new cluster 
                 // (Transitive clustering: A near B, B near C -> A, B, C cluster)
                 // This is O(N^2) but N is small (<100)
                 if cluster.iter().any(|existing| {
                     // Dist check in grid units (6 miles/hex at scale 6.0)
                     // Target: 30 miles. 
                     // Grid distance = sqrt(dx^2 + dy^2). 
                     // At 6.0 mi/hex, 30 miles is 5 hexes.
                     // But Grid X/Y are "Hex Indices". 
                     let dx = loc.x - existing.x;
                     let dy = loc.y - existing.y;
                     let dist_hexes = (dx*dx + dy*dy).sqrt();
                     let dist_miles = dist_hexes * 6.0; // FixedData was generated at 6.0
                     dist_miles <= 30.0 
                 }) {
                     cluster.push(loc);
                     added_to_cluster = true;
                     break; 
                 }
            }
            if !added_to_cluster {
                clusters.push(vec![loc]);
            }
        }

        // Merge clusters that might have bridged (Naive start, better merge logic would be union-find but simple iteration works for now)
        // Actually, the above loop is greedy and order-dependent. Let's do a quick merge pass.
        let mut merged = true;
        while merged {
            merged = false;
            let mut i = 0;
            while i < clusters.len() {
                let mut j = i + 1;
                while j < clusters.len() {
                    let should_merge = clusters[i].iter().any(|a| {
                        clusters[j].iter().any(|b| {
                             let dx = a.x - b.x;
                             let dy = a.y - b.y;
                             let dist = (dx*dx + dy*dy).sqrt() * 6.0;
                             dist <= 30.0
                        })
                    });

                    if should_merge {
                        let mut other = clusters.remove(j);
                        clusters[i].append(&mut other);
                        merged = true;
                    } else {
                        j += 1;
                    }
                }
                i += 1;
            }
        }
        
        println!("Identified {} distinct clusters.", clusters.len());

        // 3. Generate Maps for Clusters
        // Re-run fixer at 1.0 resolution for the whole world (inefficient but safe for now)
        // TODO: Optimize to only tile the bounding box
        println!("Generating high-res world data (1.0 mi/hex) for city crops...");
        let high_res_data = fixer::fix(raw_maps, Some(1.0));
        let gid_map_hr = map_gen::generate_tsj(output_dir).unwrap(); // Reuse tileset logic

        for (_idx, cluster) in clusters.iter().enumerate() {
            // Naming: Use the most significant location name
            // Logic: Vault > City > Other? Or just first one?
            // Let's sort by type priority then name length
            let mut sorted_locs = cluster.clone();
            sorted_locs.sort_by(|a, b| {
                let score_a = match a.icon_type.to_string() { "Vault" => 3, "City" => 2, _ => 1 };
                let score_b = match b.icon_type.to_string() { "Vault" => 3, "City" => 2, _ => 1 };
                if score_a != score_b { score_b.cmp(&score_a) } else { a.name.len().cmp(&b.name.len()) }
            });
            // Smart Naming Strategy
            let mut vaults = Vec::new();
            let mut others = Vec::new();

            for loc in cluster {
                let name = &loc.name;
                if name.starts_with("Vault ") {
                    // Extract numbers "Vault 13" -> "13", "Vault 31/32/33" -> "31", "32", "33"
                    let parts: Vec<&str> = name.trim_start_matches("Vault ").split(|c| c == '/' || c == ',' || c == '&' || c == ' ').collect();
                    for p in parts {
                        if !p.is_empty() { vaults.push(p.to_string()); }
                    }
                } else {
                    let abbr = match name.as_str() {
                        "Shady Sands" => "NCR",
                        "The Hub" => "Hub",
                        "The Boneyard" => "Boneyard", // or LA
                        "New Reno" => "Reno",
                        "San Francisco" => "SanFran",
                        "Vault City" => "VC", // Special case
                        "Mariposa Military Base" => "Mariposa",
                        s => s,
                    };
                    others.push(abbr.replace(" ", ""));
                }
            }
            
            vaults.sort();
            vaults.dedup();
            others.sort();
            others.dedup();

            let mut name_parts = Vec::new();
            if !vaults.is_empty() {
                name_parts.push(format!("V_{}", vaults.join("_")));
            }
            for o in others {
                name_parts.push(o);
            }
            
            let composite_name = name_parts.join("_");
            // Sanitize and truncate
            let safe_name = composite_name.replace("/", "-").replace(" ", "_");
            let safe_name = if safe_name.len() > 64 {
                safe_name[..64].to_string()
            } else {
                safe_name
            };

            let filename = format!("city_{}.tmj", safe_name);
            let png_name = format!("{}.png", safe_name);
            
            println!("Generating city map for cluster '{}' ({} locations)...", safe_name, cluster.len());
            
            // We need to CROP the high_res_data to the cluster bounds
            // 1. Calculate cluster bounds in Lat/Lon
            // We can't use .x/.y from FixedData as that's 6.0 scale.
            // We need to find the raw lat/lon of these locations. Use High Res Data locations.
            
            // Match cluster items to high_res_data items by Name
            let cluster_names: Vec<String> = cluster.iter().map(|l| l.name.clone()).collect();
            let hr_locs: Vec<&crate::schema::FixedLocation> = high_res_data.locations.iter()
                .filter(|l| cluster_names.contains(&l.name))
                .collect();

            if hr_locs.is_empty() { continue; }

            // Bounds in High Res Grid Space
            let min_x = hr_locs.iter().map(|l| l.x).fold(f32::INFINITY, |a, b| a.min(b));
            let max_x = hr_locs.iter().map(|l| l.x).fold(f32::NEG_INFINITY, |a, b| a.max(b));
            let min_y = hr_locs.iter().map(|l| l.y).fold(f32::INFINITY, |a, b| a.min(b));
            let max_y = hr_locs.iter().map(|l| l.y).fold(f32::NEG_INFINITY, |a, b| a.max(b));

            // Add 15 mile buffer (at 1.0 mi/hex, that's 15 hexes)
            let buffer = 15.0;
            let crop_min_x = (min_x - buffer).floor() as i32;
            let crop_max_x = (max_x + buffer).ceil() as i32;
            let crop_min_y = (min_y - buffer).floor() as i32;
            let crop_max_y = (max_y + buffer).ceil() as i32;

            // Create a cropped FixedMapData
            let width = (crop_max_x - crop_min_x).max(1) as u32;
            let height = (crop_max_y - crop_min_y).max(1) as u32;

            let mut cropped_tiles = Vec::new();
            for t in &high_res_data.tiles {
                if t.x >= crop_min_x && t.x < crop_max_x && t.y >= crop_min_y && t.y < crop_max_y {
                     // Rebase to 0,0
                     let mut new_t = t.clone();
                     new_t.x -= crop_min_x;
                     new_t.y -= crop_min_y;
                     cropped_tiles.push(new_t);
                }
            }

            let mut cropped_locs = Vec::new();
            for l in &high_res_data.locations {
                 if l.x >= crop_min_x as f32 && l.x <= crop_max_x as f32 &&
                    l.y >= crop_min_y as f32 && l.y <= crop_max_y as f32 {
                        let mut new_l = l.clone();
                        new_l.x -= crop_min_x as f32;
                        new_l.y -= crop_min_y as f32;
                        cropped_locs.push(new_l);
                 }
            }

            let cropped_map = crate::schema::FixedMapData {
                width,
                height,
                tiles: cropped_tiles,
                locations: cropped_locs,
                global_offset_x: 0,
                global_offset_y: 0,
            };

            // Generate TMJ
            map_gen::generate_tmj(&cities_dir, &filename, &cropped_map, &gid_map_hr, None, "../generic.tsj").unwrap();

            // Render PNG
            let tmj_path = cities_dir.join(&filename);
            let png_path = cities_dir.join(&png_name);
            
            let _ = Command::new("tmxrasterizer")
                .arg(&tmj_path)
                .arg(&png_path)
                .status();
        }
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
    
    // 1. Render Main Map (Filtered)
    let status = Command::new("tmxrasterizer")
        .arg(&map_file)
        .arg(&output_png)
        .status();

    match status {
        Ok(s) if s.success() => println!("[Rasterizer] Success: {:?}", output_png),
        Ok(s) => eprintln!("[Rasterizer] Failed with exit code: {:?}", s.code()),
        Err(e) => eprintln!("[Rasterizer] Failed to execute tmxrasterizer: {}", e),
    }

    // 2. Render All Markers Map
    let output_all_png = output_dir.join("rendered_map_all.png");
    let map_all_file = output_dir.join("map_all.tmj");
    if map_all_file.exists() {
        println!("[Rasterizer] Rendering map with all markers...");
        let _ = Command::new("tmxrasterizer")
            .arg(&map_all_file)
            .arg(&output_all_png)
            .status();
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
