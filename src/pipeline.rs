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

pub fn run_pipeline(game_extractors: Vec<Box<dyn Extractor>>, manual_extractors: Vec<Box<dyn Extractor>>, output_dir: &Path) {
    let intermediate_dir = output_dir.join("intermediate");
    let raw_dir = output_dir.join("raw");
    fs::create_dir_all(&intermediate_dir).unwrap();
    fs::create_dir_all(&raw_dir).unwrap();

    // --- STAGE 1: EXTRACTION ---
    // 1a. Extract Game Data (parallel)
    let game_results: Vec<(RawMapData, String, String, Vec<String>)> = game_extractors.par_iter().map(|extractor| {
        let (data, hash, id, links) = extract_one(extractor, &intermediate_dir);
        (data, hash, id, links)
    }).collect();

    // 1b. Extract Manual Data (parallel)
    let manual_results: Vec<(RawMapData, String, String)> = manual_extractors.par_iter().map(|extractor| {
        let (data, hash, id, _) = extract_one(extractor, &intermediate_dir);
        (data, hash, id)
    }).collect();

    // Collect marker sources for granular caching
    let mut marker_to_hashes: std::collections::HashMap<String, std::collections::HashSet<String>> = std::collections::HashMap::new();
    for (_, hash, _, links) in &game_results {
        for link in links {
            marker_to_hashes.entry(link.clone()).or_default().insert(hash.clone());
        }
    }
    for (data, hash, _) in &manual_results {
        for loc in &data.locations {
            marker_to_hashes.entry(loc.name.to_lowercase()).or_default().insert(hash.clone());
        }
    }

    // Export raw markers per extractor (all)
    for (data, _, id, _) in &game_results {
        export_markers(data, id, &raw_dir);
    }
    for (data, _, id) in &manual_results {
        export_markers(data, id, &raw_dir);
    }

    // 1c. Generate raw debug maps for manual extractors with grids (cached)
    for (data, hash, id) in &manual_results {
        if data.has_grid {
            let step_id = format!("raw_debug_{}", id);
            let mut step = CachedStep::new(&step_id, &intermediate_dir)
                .add_dependency_hash(hash)
                .add_dependency_file("src/map_gen.rs");
            
            step.run(|| {
                info!("Generating raw debug map for manual: {}", id);
                let gid_map = map_gen::generate_tsj(output_dir).expect("Failed to generate TSJ");
                let debug_filename = format!("raw_manual_{}.tmj", id);
                let _ = map_gen::generate_raw_tmj(&raw_dir, &debug_filename, data, &gid_map, "../generic.tsj");
            });
        }
    }

    // --- STAGE 2: CLUSTERING (Game Data Only) ---
    let mut parent: Vec<usize> = (0..game_results.len()).collect();
    fn find(i: usize, parent: &mut Vec<usize>) -> usize {
        if parent[i] == i { i }
        else {
            parent[i] = find(parent[i], parent);
            parent[i]
        }
    }
    fn union(i: usize, j: usize, parent: &mut Vec<usize>) {
        let root_i = find(i, parent);
        let root_j = find(j, parent);
        if root_i != root_j {
            parent[root_i] = root_j;
        }
    }

    let mut loc_to_extractors: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (idx, (_, _, _, links)) in game_results.iter().enumerate() {
        for link in links {
            loc_to_extractors.entry(link.clone()).or_default().push(idx);
        }
    }

    for extractors_for_loc in loc_to_extractors.values() {
        if extractors_for_loc.len() > 1 {
            let first = extractors_for_loc[0];
            for &other in &extractors_for_loc[1..] {
                union(first, other, &mut parent);
            }
        }
    }

    let mut clusters: std::collections::HashMap<usize, Vec<(RawMapData, String, String)>> = std::collections::HashMap::new();
    let mut cluster_members: std::collections::HashMap<usize, Vec<String>> = std::collections::HashMap::new();
    
    for (idx, (data, hash, id, _)) in game_results.into_iter().enumerate() {
        let root = find(idx, &mut parent);
        clusters.entry(root).or_default().push((data, hash, id.clone()));
        cluster_members.entry(root).or_default().push(id);
    }

    // Determine cluster names
    let mut cluster_names: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    for (&root, members) in &cluster_members {
        let name = if members.contains(&"fallout1".to_string()) && members.contains(&"fallout2".to_string()) {
            "interplay".to_string()
        } else if members.len() == 1 {
            members[0].clone()
        } else {
            format!("cluster_{}", root)
        };
        cluster_names.insert(root, name);
    }

    info!("Identified {} game clusters:", clusters.len());
    for (root, members) in &cluster_members {
        info!("  Cluster {} ({}): {:?}", root, cluster_names[root], members);
    }

    // --- STAGE 2: CLUSTER FIXING ---
    let mut clustered_raw_maps = Vec::new();
    let mut cluster_hashes = Vec::new();

    for (root_id, maps) in clusters {
        let cluster_name = &cluster_names[&root_id];
        let step_id = format!("cluster_{}", cluster_name);
        let mut cluster_step = CachedStep::new(&step_id, &intermediate_dir)
            .add_dependency_file("src/fixer.rs")
            .add_dependency_file("src/map_gen.rs");

        for (_, hash, _) in &maps {
            cluster_step = cluster_step.add_dependency_hash(hash);
        }

        let (cluster_data, cluster_hash) = cluster_step.run_with_hash(|| {
            info!("Fixing cluster: {}", cluster_name);
            let raw_cluster_maps: Vec<RawMapData> = maps.iter().map(|(d, _, _)| d.clone()).collect();
            
            // Generate debug views for raw maps in this cluster
            let gid_map = map_gen::generate_tsj(output_dir).expect("Failed to generate TSJ");
            for (data, _, id) in &maps {
                // RULE: Only non-manual OR manual with explicit grid
                let is_manual = id != "fallout1" && id != "fallout2" && id != "fallout_bos";
                if is_manual && !data.has_grid { continue; }
                
                let prefix = if is_manual { "raw_manual" } else { "raw_extracted" };
                let debug_filename = format!("{}_{}.tmj", prefix, id);
                if let Err(e) = map_gen::generate_raw_tmj(&raw_dir, &debug_filename, data, &gid_map, "../generic.tsj") {
                    warn!("Warning: Failed to generate raw debug map for {}: {}", id, e);
                }
            }

            // Cluster fix: merge individual maps into a single RawMapData for the cluster
            let fixed_raw = fixer::fix_to_raw(raw_cluster_maps);
            
            // User requested: Save clusters themselves as TMJ
            // Important: Use 6.0 mi/hex for the visual debug TMJ so it's not multi-gigabyte/invalid
            let debug_fixed_for_tmj = fixer::fix(vec![fixed_raw.clone()], vec![], Some(6.0), None);
            let cluster_tmj_name = format!("cluster_{}.tmj", cluster_name);
            if let Err(e) = map_gen::generate_tmj(output_dir, &cluster_tmj_name, &debug_fixed_for_tmj, &gid_map, None, "generic.tsj") {
                warn!("Warning: Failed to generate cluster TMJ for {}: {}", cluster_name, e);
            }

            fixed_raw
        });

        clustered_raw_maps.push(cluster_data);
        cluster_hashes.push(cluster_hash);
    }

    // --- STAGE 3: GLOBAL FIXER ---
    let mut global_step = CachedStep::new("global_fixed_data", &intermediate_dir)
        .add_dependency_file("src/fixer.rs");
        
    for h in &cluster_hashes {
        global_step = global_step.add_dependency_hash(h);
    }
    // Note: Manual maps are also dependencies, but they are relatively small and don't affect alignment.
    // We add their hashes too for correctness.
    for (_, hash, _) in &manual_results {
        global_step = global_step.add_dependency_hash(hash);
    }
    
    let manual_raw_maps: Vec<RawMapData> = manual_results.iter().map(|(d, _, _)| d.clone()).collect();
    let (final_fixed_data, combined_global_hash) = global_step.run_with_hash(|| {
         fixer::fix(clustered_raw_maps.clone(), manual_raw_maps.clone(), None, None) 
    });

    // --- STAGE 4: BUILDER ---
    let builder_source = "src/map_gen.rs";
    
    // We return () from the builder step as it just generates files
    let step = CachedStep::new("builder", &intermediate_dir)
        .add_dependency_file(builder_source)
        .add_dependency_hash(&combined_global_hash);

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
        // 2. Cluster Locations (Deterministic and fast)
        let city_clusters = crate::clustering::cluster_locations(&city_targets, 30.0);
        info!("Found {} city/vault/landmark targets, grouped into {} clusters.", city_targets.len(), city_clusters.len());

        // 3. Generate Maps for Clusters (Parallelized & Granularly Cached)
        city_clusters.par_iter().for_each(|cluster| {
            // A) Naming: Use the most significant location names to build a safe filename
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
                    for p in parts { if !p.is_empty() { vaults.push(p.to_string()); } }
                } else {
                    let abbr = match name.as_str() {
                        "Shady Sands" => "NCR", "The Hub" => "Hub", "The Boneyard" => "Boneyard",
                        "New Reno" => "Reno", "San Francisco" => "SanFran", "Vault City" => "VC",
                        "Mariposa Military Base" => "Mariposa", s => s,
                    };
                    others.push(abbr.replace(" ", ""));
                }
            }
            vaults.sort(); vaults.dedup();
            others.sort(); others.dedup();

            let mut name_parts = Vec::new();
            if !vaults.is_empty() { name_parts.push(format!("V_{}", vaults.join("_"))); }
            for o in others { name_parts.push(o); }
            let composite_name = name_parts.join("_");
            let safe_name = composite_name.replace("/", "-").replace(" ", "_");
            let safe_name = if safe_name.len() > 64 { safe_name[..64].to_string() } else { safe_name };

            // B) Granular Dependencies
            let cluster_names: Vec<String> = cluster.iter().map(|l| l.name.clone()).collect();
            let mut cluster_dep_hashes = std::collections::HashSet::new();
            for name in &cluster_names {
                if let Some(hashes) = marker_to_hashes.get(&name.to_lowercase()) {
                    for h in hashes { cluster_dep_hashes.insert(h.clone()); }
                }
            }
            
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            cluster_names.hash(&mut hasher);
            let cluster_content_hash = hasher.finish();
            
            let city_step_id = format!("city_map_{}_{:x}", safe_name, cluster_content_hash);
            let mut city_cached_step = CachedStep::new(&city_step_id, &intermediate_dir)
                .add_dependency_file("src/map_gen.rs")
                .add_dependency_file("src/fixer.rs")
                .add_dependency_file("src/clustering.rs");
            let mut cluster_dep_hashes: Vec<String> = cluster_dep_hashes.into_iter().collect();
            cluster_dep_hashes.sort();
            
            for h in cluster_dep_hashes { city_cached_step = city_cached_step.add_dependency_hash(&h); }

            // C) Run Cached Generation
            city_cached_step.run(|| {
                info!("Generating city map for cluster '{}' ({} locations)...", safe_name, cluster.len());
                let gid_map_hr = map_gen::generate_tsj(output_dir).unwrap();

                // D) Use Bounds-Limited Fixer (Fast Reconstruction)
                let min_lat = cluster.iter().map(|l| l.real_lat.unwrap_or(0.0)).fold(f32::INFINITY, |a, b| a.min(b)) - 0.3;
                let max_lat = cluster.iter().map(|l| l.real_lat.unwrap_or(0.0)).fold(f32::NEG_INFINITY, |a, b| a.max(b)) + 0.3;
                let min_lon = cluster.iter().map(|l| l.real_lon.unwrap_or(0.0)).fold(f32::INFINITY, |a, b| a.min(b)) - 0.3;
                let max_lon = cluster.iter().map(|l| l.real_lon.unwrap_or(0.0)).fold(f32::NEG_INFINITY, |a, b| a.max(b)) + 0.3;
                
                let bounds = fixer::BoundingBox { min_lat, max_lat, min_lon, max_lon };
                let manual_raw_maps: Vec<RawMapData> = manual_results.iter().map(|(d, _, _)| d.clone()).collect();
                let high_res_data = fixer::fix(clustered_raw_maps.clone(), manual_raw_maps, Some(1.0), Some(bounds));

                // E) Crop and Export
                let hr_locs: Vec<&crate::schema::FixedLocation> = high_res_data.locations.iter()
                    .filter(|l| cluster_names.contains(&l.name)).collect();
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

                let mut cropped_tiles = Vec::new();
                for t in &high_res_data.tiles {
                    if t.x >= crop_min_x && t.x < crop_max_x && t.y >= crop_min_y && t.y < crop_max_y {
                        let mut new_t = t.clone(); new_t.x -= crop_min_x; new_t.y -= crop_min_y;
                        cropped_tiles.push(new_t);
                    }
                }
                let mut cropped_locs = Vec::new();
                for l in &high_res_data.locations {
                    if l.x >= crop_min_x as f32 && l.x <= crop_max_x as f32 && l.y >= crop_min_y as f32 && l.y <= crop_max_y as f32 {
                        let mut new_l = l.clone(); new_l.x -= crop_min_x as f32; new_l.y -= crop_min_y as f32;
                        cropped_locs.push(new_l);
                    }
                }

                let cropped_map = crate::schema::FixedMapData {
                    width: (crop_max_x - crop_min_x).max(1) as u32,
                    height: (crop_max_y - crop_min_y).max(1) as u32,
                    tiles: cropped_tiles, locations: cropped_locs,
                    global_offset_x: 0, global_offset_y: 0,
                };

                let filename = format!("city_{}.tmj", safe_name);
                map_gen::generate_tmj(&cities_dir, &filename, &cropped_map, &gid_map_hr, None, "../generic.tsj").unwrap();
                let _ = Command::new("tmxrasterizer").arg(cities_dir.join(&filename)).arg(cities_dir.join(format!("{}.png", safe_name))).status();
            });
        });
    }

    // --- STAGE 3.5: MARKER EXPORT ---
     let marker_file = output_dir.join("markers.json");
     if let Ok(json) = serde_json::to_string_pretty(&final_fixed_data.locations) {
         fs::write(&marker_file, json).expect("Failed to write markers.json");
         info!("[Pipeline] Exported {} markers to markers.json", final_fixed_data.locations.len());
     }

    // --- STAGE 4: RASTERIZER ---
    info!("[Rasterizer] Running global render pass...");
    
    let render_targets = vec![
        (output_dir, "map.tmj", "rendered_map.png"),
        (output_dir, "map_all.tmj", "rendered_map_all.png"),
    ];

    for (dir, tmj, png) in render_targets {
        let tmj_path = dir.join(tmj);
        let png_path = dir.join(png);
        if tmj_path.exists() {
            let _ = Command::new("tmxrasterizer")
                .arg(&tmj_path)
                .arg(&png_path)
                .status();
        }
    }

    render_all_in_dir(output_dir);
    render_all_in_dir(&raw_dir);

    info!("Done! All outputs stored in {}", output_dir.display());
}

fn extract_one(extractor: &Box<dyn Extractor>, intermediate_dir: &Path) -> (RawMapData, String, String, Vec<String>) {
    let id = extractor.get_id().to_string();
    let source_file = extractor.get_code_path();
    
    let mut step = CachedStep::new(&id, intermediate_dir)
        .add_dependency_file(source_file);
    
    let config_path = "input/config/paths.json";
    if Path::new(config_path).exists() {
        step = step.add_dependency_file(config_path);
    }
    
    for path in extractor.get_source_paths() {
        step = step.add_dependency_file(&path);
    }
    
    let (data, hash) = step.run_with_hash(|| {
        extractor.extract()
    });
    
    // Collect links for clustering
    let mut links = Vec::new();
    for loc in &data.locations {
        links.push(loc.name.to_lowercase());
        if let Some(anchor) = &loc.anchor {
            links.push(anchor.to_lowercase());
        }
    }
    
    (data, hash, id, links)
}

fn export_markers(data: &RawMapData, id: &str, raw_dir: &Path) {
    let marker_export_path = raw_dir.join(format!("{}_markers.json", id));
    if let Ok(json) = serde_json::to_string_pretty(&data.locations) {
        let _ = fs::write(marker_export_path, json).expect("Failed to write markers");
    }
}

fn render_all_in_dir(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "tmj" {
                    let name = path.file_name().unwrap().to_string_lossy();
                    if name.starts_with("cluster_") || name.starts_with("raw_extracted_") || name.starts_with("raw_manual_") {
                        let png_path = path.with_extension("png");
                        info!("[Rasterizer] Rendering debug/cluster map: {}", name);
                        let _ = Command::new("tmxrasterizer")
                            .arg(&path)
                            .arg(&png_path)
                            .status();
                    }
                }
            }
        }
    }
}
