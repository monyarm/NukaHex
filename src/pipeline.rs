use std::fs;
use std::path::Path;
use rayon::prelude::*;
use crate::caching::CachedStep;
use crate::extractors::Extractor;
use crate::schema::RawMapData;
use crate::{map_gen, fixer};
use std::process::Command;

use log::{info, warn, error};

// ... imports

pub fn run_pipeline(extractors: Vec<Box<dyn Extractor>>, output_dir: &Path) {
    let intermediate_dir = output_dir.join("intermediate");
    fs::create_dir_all(&intermediate_dir).unwrap();

    let mut raw_maps = Vec::new();
    let mut extractor_hashes = Vec::new();

    // --- STAGE 1: EXTRACTORS ---
    // --- STAGE 1: EXTRACTORS ---
    // Use Rayon for parallel processing
    // --- STAGE 1: EXTRACTORS ---
    // Use Rayon for parallel processing
    let results: Vec<(RawMapData, String)> = extractors.par_iter().map(|extractor| {
        let id = extractor.get_id();
        let source_file = extractor.get_code_path();
        
        let mut step = CachedStep::new(id, &intermediate_dir)
            .add_dependency_file(source_file);
        
        let config_path = "input/config/paths.json";
        if Path::new(config_path).exists() {
            step = step.add_dependency_file(config_path);
        }
        
        for path in extractor.get_source_paths() {
            step = step.add_dependency_file(&path);
        }
        
        step.run_with_hash(|| {
            extractor.extract()
        })
    }).collect();

    for (data, hash) in results {
        raw_maps.push(data);
        extractor_hashes.push(hash);
    }

    // --- STAGE 2: FIXER ---
    let fixer_source = "src/fixer.rs";
    let mut step = CachedStep::new("fixed_data", &intermediate_dir)
        .add_dependency_file(fixer_source);
        
    for h in &extractor_hashes {
        step = step.add_dependency_hash(h);
    }
    
    let (final_fixed_data, combined_fixer_hash) = step.run_with_hash(|| {
         // Generate debug raw view for EACH map
         // We do this inside the closure so it only happens when cache misses
         let gid_map = map_gen::generate_tsj(output_dir).unwrap(); 
         
         for (i, raw_data) in raw_maps.iter().enumerate() {
             let id = if i < extractors.len() { extractors[i].get_id() } else { "unknown" };
             
             // Generate debug map if it's not the generic 'manual' ID (it's the filename)
             // or if it has a grid (the user explicitly asked for this)
             let is_manual_base = id == "manual";
             if is_manual_base && !raw_data.has_grid { continue; }
             
             // Additional check: Skip manual files completely if they don't have a grid
             // unless they are the specific 'ncr' file we are testing (or future ones with grids)
             if id != "fallout1" && id != "fallout2" && !raw_data.has_grid {
                 continue;
             }
             
             let debug_filename = format!("raw_extracted_{}.tmj", id);
             let _debug_path = output_dir.join(&debug_filename);
             
             if let Err(e) = map_gen::generate_raw_tmj(output_dir, &debug_filename, raw_data, &gid_map) {
                 warn!("Warning: Failed to generate raw debug map for {}: {}", id, e);
             } else {
                 info!("Generated debug map: {}", debug_filename);
             }
         }

         fixer::fix(raw_maps.clone(), None, None) // Default 6.0 miles/hex resolution, no bounds limit
    });

    // --- STAGE 3: BUILDER ---
    // For now, we'll adapt our old map_gen to take FixedMapData later.
    // Hash builder code + fixed data input
    // --- STAGE 3: BUILDER ---
    // For now, we'll adapt our old map_gen to take FixedMapData later.
    // Hash builder code + fixed data input
    let builder_source = "src/map_gen.rs";
    
    // We return () from the builder step as it just generates files
    let step = CachedStep::new("builder", &intermediate_dir)
        .add_dependency_file(builder_source)
        .add_dependency_hash(&combined_fixer_hash);

    step.run(|| {
        info!("[Builder] Running...");
        // Call map generator with FixedMapData
        let gid_map = map_gen::generate_tsj(output_dir).unwrap();
        
        // 1. Generate map with ALL markers
        map_gen::generate_tmj(output_dir, "map_all.tmj", &final_fixed_data, &gid_map, None, "generic.tsj").unwrap();

        // 2. Generate map with FILTERED markers (Cities, Vaults, Monuments)
        // Note: Filters are case-insensitive
        let allowed_types = vec!["City", "Vault", "Monument", "Settlement", "Ruins", "Landmark"]; 
        map_gen::generate_tmj(output_dir, "map.tmj", &final_fixed_data, &gid_map, Some(&allowed_types), "generic.tsj").unwrap();
    });


    // --- STAGE 3.1: CITY MAP GENERATION ---
    info!("[Pipeline] Generating City Maps...");
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
        let city_step = CachedStep::new("city_maps", &intermediate_dir)
             .add_dependency_file("src/map_gen.rs") // Uses map_gen
             .add_dependency_file("src/fixer.rs")   // Uses fixer logic (re-run)
             .add_dependency_file("src/clustering.rs"); // Uses clustering

        // Add extractor hashes as this step depends on raw_maps logic which comes from extractors
        let mut city_step = city_step;
        for h in &extractor_hashes {
            city_step = city_step.add_dependency_hash(h);
        }

        city_step.run(|| {
            info!("Found {} city/vault/landmark targets for detailed maps.", city_targets.len());

            // 2. Cluster Locations
            let clusters = crate::clustering::cluster_locations(&city_targets, 30.0);
            
            info!("Identified {} distinct clusters.", clusters.len());

            // 3. Generate High-Res World Data (1.0 mi/hex)
            // Since we've parallelized fixer::fix, this is now much faster!
            info!("Generating high-res world data (1.0 mi/hex) for city crops...");
            let high_res_data = fixer::fix(raw_maps.clone(), Some(1.0), None);
            let gid_map_hr = map_gen::generate_tsj(output_dir).unwrap(); // Reuse tileset logic

            // 4. Generate Maps for Clusters (Parallelized)
            clusters.par_iter().for_each(|cluster| {
                // Naming: Use the most significant location name
                let mut sorted_locs = cluster.clone();
                sorted_locs.sort_by(|a, b| {
                    let score_a = match a.icon_type { crate::schema::IconType::Vault => 3, crate::schema::IconType::City => 2, _ => 1 };
                    let score_b = match b.icon_type { crate::schema::IconType::Vault => 3, crate::schema::IconType::City => 2, _ => 1 };
                    if score_a != score_b { score_b.cmp(&score_a) } else { a.name.len().cmp(&b.name.len()) }
                });
                
                let mut vaults = Vec::new();
                let mut others = Vec::new();

                for loc in cluster {
                    let name = &loc.name;
                    if name.starts_with("Vault ") {
                        let parts: Vec<&str> = name.trim_start_matches("Vault ").split(|c| c == '/' || c == ',' || c == '&' || c == ' ').collect();
                        for p in parts {
                            if !p.is_empty() { vaults.push(p.to_string()); }
                        }
                    } else {
                        let abbr = match name.as_str() {
                            "Shady Sands" => "NCR",
                            "The Hub" => "Hub",
                            "The Boneyard" => "Boneyard",
                            "New Reno" => "Reno",
                            "San Francisco" => "SanFran",
                            "Vault City" => "VC",
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
                let safe_name = composite_name.replace("/", "-").replace(" ", "_");
                let safe_name = if safe_name.len() > 64 { safe_name[..64].to_string() } else { safe_name };

                let filename = format!("city_{}.tmj", safe_name);
                let png_name = format!("{}.png", safe_name);
                
                info!("Generating city map for cluster '{}' ({} locations)...", safe_name, cluster.len());
                
                let cluster_names: Vec<String> = cluster.iter().map(|l| l.name.clone()).collect();
                let hr_locs: Vec<&crate::schema::FixedLocation> = high_res_data.locations.iter()
                    .filter(|l| cluster_names.contains(&l.name))
                    .collect();

                if hr_locs.is_empty() { return; }

                let min_x = hr_locs.iter().map(|l| l.x).fold(f32::INFINITY, |a, b| a.min(b));
                let max_x = hr_locs.iter().map(|l| l.x).fold(f32::NEG_INFINITY, |a, b| a.max(b));
                let min_y = hr_locs.iter().map(|l| l.y).fold(f32::INFINITY, |a, b| a.min(b));
                let max_y = hr_locs.iter().map(|l| l.y).fold(f32::NEG_INFINITY, |a, b| a.max(b));

                let buffer = 15.0;
                let crop_min_x = (min_x - buffer).floor() as i32;
                let crop_max_x = (max_x + buffer).ceil() as i32;
                let crop_min_y = (min_y - buffer).floor() as i32;
                let crop_max_y = (max_y + buffer).ceil() as i32;

                let width = (crop_max_x - crop_min_x).max(1) as u32;
                let height = (crop_max_y - crop_min_y).max(1) as u32;

                let mut cropped_tiles = Vec::new();
                for t in &high_res_data.tiles {
                    if t.x >= crop_min_x && t.x < crop_max_x && t.y >= crop_min_y && t.y < crop_max_y {
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

                map_gen::generate_tmj(&cities_dir, &filename, &cropped_map, &gid_map_hr, None, "../generic.tsj").unwrap();

                let tmj_path = cities_dir.join(&filename);
                let png_path = cities_dir.join(&png_name);
                
                let _ = Command::new("tmxrasterizer")
                    .arg(&tmj_path)
                    .arg(&png_path)
                    .status();
            });
        }); // End of city_step.run
    }

    // --- STAGE 3.5: MARKER EXPORT ---
     let marker_file = output_dir.join("markers.json");
     if let Ok(json) = serde_json::to_string_pretty(&final_fixed_data.locations) {
         fs::write(&marker_file, json).expect("Failed to write markers.json");
         info!("[Pipeline] Exported {} markers to markers.json", final_fixed_data.locations.len());
     }

    // --- STAGE 4: RASTERIZER ---
    let output_png = output_dir.join("rendered_map.png");
    let map_file = output_dir.join("map.tmj");

    info!("[Rasterizer] Running...");
    
    // 1. Render Main Map (Filtered)
    let status = Command::new("tmxrasterizer")
        .arg(&map_file)
        .arg(&output_png)
        .status();

    match status {
        Ok(s) if s.success() => info!("[Rasterizer] Success: {:?}", output_png),
        Ok(s) => error!("[Rasterizer] Failed with exit code: {:?}", s.code()),
        Err(e) => error!("[Rasterizer] Failed to execute tmxrasterizer: {}", e),
    }

    // 2. Render All Markers Map
    let output_all_png = output_dir.join("rendered_map_all.png");
    let map_all_file = output_dir.join("map_all.tmj");
    if map_all_file.exists() {
        info!("[Rasterizer] Rendering map with all markers...");
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
                     info!("[Rasterizer] Rendering debug map: {}", name);
                     let _ = Command::new("tmxrasterizer")
                        .arg(&path)
                        .arg(&png_path)
                        .status();
                }
            }
        }
    }

}
