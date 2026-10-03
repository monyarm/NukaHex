use std::fs;
use std::path::Path;
use rayon::prelude::*;
use crate::caching::CachedStep;
use crate::extractors::Extractor;
use crate::schema::RawMapData;
use crate::{map_gen, fixer};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use log::{info, warn, error};

/// Every source whose logic shapes fixed (warped/merged) data; cached fixer outputs depend on all.
const FIXER_SOURCES: &[&str] = &["src/fixer.rs", "src/rubbersheeting.rs", "src/geocoding.rs", "src/schema.rs"];

/// Runs `f`, logging how long it took under `[timing]`.
fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    info!("[timing] {:<28} {:>8.2}s", label, start.elapsed().as_secs_f64());
    out
}

pub fn run_pipeline(game_extractors: Vec<Box<dyn Extractor>>, manual_extractors: Vec<Box<dyn Extractor>>, output_dir: &Path, target_cluster: Option<String>) {
    let intermediate_dir = output_dir.join("intermediate");
    let raw_dir = output_dir.join("raw");
    fs::create_dir_all(&intermediate_dir).unwrap();
    fs::create_dir_all(&raw_dir).unwrap();

    // --- STAGE 1: EXTRACTION ---
    // 1a. Extract Game Data (parallel)
    let game_results: Vec<(RawMapData, String, String, Vec<String>)> = timed("extract games", ||
        game_extractors.par_iter().map(|extractor| extract_one(extractor, &intermediate_dir)).collect());

    // 1b. Extract Manual Data (parallel)
    let manual_results: Vec<(RawMapData, String, String)> = timed("extract manual", ||
        manual_extractors.par_iter().map(|extractor| {
            let (data, hash, id, _) = extract_one(extractor, &intermediate_dir);
            (data, hash, id)
        }).collect());

    // Every (tmj, png) this run produces; rendered once at the end (never stale files).
    let renders: Mutex<Vec<(PathBuf, PathBuf)>> = Mutex::new(Vec::new());
    let add_render = |tmj: PathBuf, png: PathBuf| renders.lock().unwrap().push((tmj, png));

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
        if let Some(target) = &target_cluster {
            if !id.contains(target) && !target.contains(id) { continue; }
        }
        if data.has_grid {
            let tmj = raw_dir.join(format!("raw_manual_{}.tmj", id));
            add_render(tmj.clone(), tmj.with_extension("png"));
            let step_id = format!("raw_debug_{}", id);
            let step = CachedStep::new(&step_id, &intermediate_dir)
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

    if let Some(target) = &target_cluster {
        clusters.retain(|root_id, _| {
            let matches_name = cluster_names.get(root_id).map_or(false, |s| s.contains(target) || target.contains(s));
            let matches_members = cluster_members.get(root_id).map_or(false, |members| members.iter().any(|m| m.contains(target) || target.contains(m)));
            matches_name || matches_members
        });
        if clusters.is_empty() {
            warn!("Warning: No clusters matched the target '{}'", target);
        }
    }

    info!("Identified {} game clusters to process:", clusters.len());
    for (root, _) in &clusters {
        info!("  Cluster {} ({}): {:?}", root, cluster_names[root], cluster_members[root]);
    }

    // --- STAGE 2.5: CLUSTER FIXING ---
    // Clusters are independent: fix them in parallel, in a fixed (name) order.
    let mut clusters: Vec<_> = clusters.into_iter().collect();
    clusters.sort_by(|a, b| cluster_names[&a.0].cmp(&cluster_names[&b.0]));
    for (root_id, maps) in &clusters {
        for (data, _, id) in maps {
            let is_manual = data.coordinate_scale <= 1.01; // the fixer's own definition of game data
            if is_manual && !data.has_grid { continue; }
            let prefix = if is_manual { "raw_manual" } else { "raw_extracted" };
            let tmj = raw_dir.join(format!("{}_{}.tmj", prefix, id));
            add_render(tmj.clone(), tmj.with_extension("png"));
        }
        let tmj = output_dir.join(format!("cluster_{}.tmj", cluster_names[root_id]));
        add_render(tmj.clone(), tmj.with_extension("png"));
    }
    let fixed_clusters: Vec<(RawMapData, String)> = timed("fix clusters", || clusters.par_iter().map(|(root_id, maps)| {
        let cluster_name = &cluster_names[root_id];
        let step_id = format!("cluster_{}", cluster_name);
        let mut cluster_step = CachedStep::new(&step_id, &intermediate_dir)
            .add_dependency_files(FIXER_SOURCES)
            .add_dependency_file("src/map_gen.rs");

        for (_, hash, _) in maps {
            cluster_step = cluster_step.add_dependency_hash(hash);
        }

        let (cluster_data, cluster_hash) = cluster_step.run_with_hash(|| {
            info!("Fixing cluster: {}", cluster_name);
            let raw_cluster_maps: Vec<RawMapData> = maps.iter().map(|(d, _, _)| d.clone()).collect();
            
            // Generate debug views for raw maps in this cluster
            let gid_map = map_gen::generate_tsj(output_dir).expect("Failed to generate TSJ");
            for (data, _, id) in maps {
                // RULE: Only non-manual OR manual with explicit grid
                let is_manual = data.coordinate_scale <= 1.01; // the fixer's own definition of game data
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

        // Write out per-cluster markers
        let cluster_marker_file = output_dir.join(format!("cluster_{}_markers.json", cluster_name));
        if let Ok(json) = serde_json::to_string_pretty(&cluster_data.locations) {
            let _ = fs::write(&cluster_marker_file, json);
        }

        // Write out per-cluster warnings if they exist
        if !cluster_data.warnings.is_empty() {
            let log_file = output_dir.join(format!("cluster_{}.log", cluster_name));
            let mut log_content = String::new();
            for w in &cluster_data.warnings {
                log_content.push_str(w);
                log_content.push('\n');
            }
            let _ = fs::write(&log_file, log_content);
        }

        (cluster_data, cluster_hash)
    }).collect());
    let (clustered_raw_maps, cluster_hashes): (Vec<RawMapData>, Vec<String>) = fixed_clusters.into_iter().unzip();

    // --- STAGE 3: GLOBAL FIXER ---
    if let Some(target) = &target_cluster {
        info!("Skipping global fixing for single-cluster run: {}", target);
        timed("render", || render_outputs(renders.into_inner().unwrap(), &intermediate_dir));
        info!("Done! Cluster outputs stored in {}", output_dir.display());
        return;
    }

    let mut global_step = CachedStep::new("global_fixed_data", &intermediate_dir)
        .add_dependency_files(FIXER_SOURCES);
        
    for h in &cluster_hashes {
        global_step = global_step.add_dependency_hash(h);
    }
    // Note: Manual maps are also dependencies, but they are relatively small and don't affect alignment.
    // We add their hashes too for correctness.
    for (_, hash, _) in &manual_results {
        global_step = global_step.add_dependency_hash(hash);
    }
    
    let manual_raw_maps: Vec<RawMapData> = manual_results.iter().map(|(d, _, _)| d.clone()).collect();
    let (final_fixed_data, combined_global_hash) = timed("fix global", || global_step.run_with_hash(|| {
         fixer::fix(clustered_raw_maps.clone(), manual_raw_maps.clone(), None, None)
    }));

    // --- STAGE 4: BUILDER ---
    let builder_source = "src/map_gen.rs";
    
    // We return () from the builder step as it just generates files
    let step = CachedStep::new("builder", &intermediate_dir)
        .add_dependency_file(builder_source)
        .add_dependency_hash(&combined_global_hash);

    add_render(output_dir.join("map_all.tmj"), output_dir.join("rendered_map_all.png"));
    add_render(output_dir.join("map.tmj"), output_dir.join("rendered_map.png"));
    step.run(|| {
        info!("[Builder] Running...");
        // Call map generator with FixedMapData
        let gid_map = map_gen::generate_tsj(output_dir).unwrap();
        
        // 1. Generate map with ALL markers
        map_gen::generate_tmj(output_dir, "map_all.tmj", &final_fixed_data, &gid_map, None, "generic.tsj").unwrap();

        // 2. Generate map with FILTERED markers (Cities, Vaults, Monuments)
        // Note: Filters are case-insensitive
        let allowed_types = vec!["City", "Vault", "Monument", "Settlement", "Landmark"]; 
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
            matches!(t, "City" | "Vault" | "Monument" | "Settlement" | "Landmark")
        })
        .collect();

    if !city_targets.is_empty() {
        // 2. Cluster Locations (Deterministic and fast)
        let city_clusters = crate::clustering::cluster_locations(&city_targets, 30.0);
        info!("Found {} city/vault/landmark targets, grouped into {} clusters.", city_targets.len(), city_clusters.len());

        // One 1 mi fix over the union of all city areas, computed only if some city map is stale.
        // Every grid sits on the shared `fixer::to_grid` lattice, so cropping it equals a per-city fix.
        let union = city_clusters.iter().flatten().fold(None, |acc: Option<fixer::BoundingBox>, l| {
            let (lat, lon) = (l.real_lat.unwrap_or(0.0), l.real_lon.unwrap_or(0.0));
            let b = acc.unwrap_or(fixer::BoundingBox { min_lat: lat, max_lat: lat, min_lon: lon, max_lon: lon });
            Some(fixer::BoundingBox { min_lat: b.min_lat.min(lat), max_lat: b.max_lat.max(lat), min_lon: b.min_lon.min(lon), max_lon: b.max_lon.max(lon) })
        }).map(|b| fixer::BoundingBox { min_lat: b.min_lat - 0.3, max_lat: b.max_lat + 0.3, min_lon: b.min_lon - 0.3, max_lon: b.max_lon + 0.3 });
        let high_res: OnceLock<crate::schema::FixedMapData> = OnceLock::new();
        let high_res_data = || high_res.get_or_init(|| timed("fix city area (1 mi)", ||
            fixer::fix(clustered_raw_maps.clone(), manual_raw_maps.clone(), Some(1.0), union)));

        // 3. Generate Maps for Clusters (Parallelized & Granularly Cached)
        timed("city maps", || city_clusters.par_iter().for_each(|cluster| {
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
                .add_dependency_files(FIXER_SOURCES)
                .add_dependency_file("src/clustering.rs");
            let mut cluster_dep_hashes: Vec<String> = cluster_dep_hashes.into_iter().collect();
            cluster_dep_hashes.sort();
            
            for h in cluster_dep_hashes { city_cached_step = city_cached_step.add_dependency_hash(&h); }

            let tmj = cities_dir.join(format!("city_{}.tmj", safe_name));
            add_render(tmj.clone(), cities_dir.join(format!("{}.png", safe_name)));

            // C) Run Cached Generation
            city_cached_step.run(|| {
                info!("Generating city map for cluster '{}' ({} locations)...", safe_name, cluster.len());
                let gid_map_hr = map_gen::generate_tsj(output_dir).unwrap();

                // D) Crop from the shared high-res fix
                let high_res_data = high_res_data();

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
                    warnings: high_res_data.warnings.clone(),
                    world_bounds: None,
                };

                let filename = format!("city_{}.tmj", safe_name);
                map_gen::generate_tmj(&cities_dir, &filename, &cropped_map, &gid_map_hr, None, "../generic.tsj").unwrap();
            });
        }));
    }

    // --- STAGE 3.5: MARKER EXPORT ---
     let marker_file = output_dir.join("global_markers.json");
     if let Ok(json) = serde_json::to_string_pretty(&final_fixed_data.locations) {
         fs::write(&marker_file, json).expect("Failed to write global_markers.json");
         info!("[Pipeline] Exported {} global markers to global_markers.json", final_fixed_data.locations.len());
     }
     
     if !manual_raw_maps.is_empty() {
         let manual_marker_file = output_dir.join("manual_markers.json");
         let manual_locs: Vec<_> = manual_raw_maps.iter().flat_map(|m| m.locations.clone()).collect();
         if let Ok(json) = serde_json::to_string_pretty(&manual_locs) {
             let _ = fs::write(&manual_marker_file, json);
             info!("[Pipeline] Exported {} manual markers to manual_markers.json", manual_locs.len());
         }
     }

    // --- STAGE 4: RASTERIZER ---
    timed("render", || render_outputs(renders.into_inner().unwrap(), &intermediate_dir));

    info!("Done! All outputs stored in {}", output_dir.display());
}

fn extract_one(extractor: &Box<dyn Extractor>, intermediate_dir: &Path) -> (RawMapData, String, String, Vec<String>) {
    let id = extractor.get_id().to_string();
    let source_file = extractor.get_code_path();
    
    let mut step = CachedStep::new(&id, intermediate_dir)
        .add_dependency_file(source_file)
        .add_dependency_file("src/schema.rs")
        .add_dependency_file("src/frm.rs");
    
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

/// Renders each (tmj, png) in parallel, skipping those whose TMJ and renderer are unchanged
/// since the PNG was last written.
fn render_outputs(mut targets: Vec<(PathBuf, PathBuf)>, intermediate_dir: &Path) {
    targets.sort();
    targets.dedup();
    let renderer_hash = crate::caching::compute_source_hash("src/render.rs");
    targets.par_iter().for_each(|(tmj, png)| {
        if !tmj.exists() { return; }
        let hash = format!("{}{}", crate::caching::compute_source_hash(&tmj.to_string_lossy()), renderer_hash);
        let stamp = intermediate_dir.join(format!("render_{}.hash", png.to_string_lossy().replace(['/', '\\'], "_")));
        if png.exists() && fs::read_to_string(&stamp).is_ok_and(|h| h == hash) { return; }
        match crate::render::render_tmj(tmj, png) {
            Ok(()) => { let _ = fs::write(&stamp, hash); info!("[Render] {}", png.display()); }
            Err(e) => warn!("[Render] {} failed: {}", tmj.display(), e),
        }
    });
}
