use crate::tiled_structs::*;
use crate::schema::FixedMapData;
use std::fs;
use std::path::Path;
use std::collections::HashMap;
use serde_json;

fn resolve_asset_path(filename: &str) -> Option<String> {
    if Path::new("input").join(filename).exists() {
        return Some(format!("../../input/{}", filename));
    }
    if Path::new("input/icons").join(filename).exists() {
        return Some(format!("../../input/icons/{}", filename));
    }
    if Path::new("output/tiles").join(filename).exists() {
        return Some(format!("../tiles/{}", filename));
    }
    None
}

pub fn generate_tsj(output_dir: &Path) -> std::io::Result<HashMap<String, u32>> {
    let mut tiles = Vec::new();
    let mut gid_map = HashMap::new();
    let mut next_id = 0;

    let terrain_assets = vec!["grass.png", "water.png", "sand.png", "rock.png", "mountain.png", "city.png", "unknown.png"];
    for filename in terrain_assets {
        if let Some(path) = resolve_asset_path(filename) {
             // 66x76 (Mathematical standard for Side 38)
             tiles.push(TileDef { id: next_id, image: path, imageheight: 76, imagewidth: 66 });
             let stem = Path::new(filename).file_stem().unwrap().to_str().unwrap().to_lowercase();
             // Namespace terrain tiles to avoid collisions with icon types (e.g. "City")
             gid_map.insert(format!("terrain_{}", stem), next_id + 1);
             next_id += 1;
        }
    }
    
    // ... (icons loading same as before, no changes needed) ...
    let icons_dir = Path::new("input/icons");
    if icons_dir.exists() {
        for entry in fs::read_dir(icons_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "png" {
                         let filename = path.file_name().unwrap().to_str().unwrap();
                         if let Some(rel_path) = resolve_asset_path(filename) {
                             tiles.push(TileDef { id: next_id, image: rel_path, imageheight: 76, imagewidth: 66 });
                             let stem = path.file_stem().unwrap().to_str().unwrap().to_string(); 
                             gid_map.insert(stem, next_id + 1); // Icons keep original names
                             // next_id += 1;
                         }
                    }
                }
            }
        }
    }

    if !gid_map.contains_key("marker") {
         if let Some(path) = resolve_asset_path("marker.png") {
             tiles.push(TileDef { id: next_id, image: path, imageheight: 76, imagewidth: 66 });
             gid_map.insert("marker".to_string(), next_id + 1);
        }
    }
    
    // ... (Tileset struct creation same as before) ...
    let tileset = Tileset {
        columns: 0,
        image: None,
        imageheight: None,
        imagewidth: None,
        margin: 0,
        name: "GenericTerrain".to_string(),
        spacing: 0,
        tilecount: tiles.len() as u32,
        tileheight: 76, 
        tilewidth: 66, 
        tileset_type: "tileset".to_string(),
        version: "1.10".to_string(),
        tiledversion: "1.10.2".to_string(),
        tiles: Some(tiles),
        tileoffset: None, 
    };

    let json = serde_json::to_string_pretty(&tileset)?;
    fs::write(output_dir.join("generic.tsj"), json)?;
    println!("Generated tileset generic.tsj");
    Ok(gid_map)
}

pub fn generate_tmj(output_dir: &Path, filename: &str, map_data: &FixedMapData, gid_map: &HashMap<String, u32>, allowed_types: Option<&[&str]>, tileset_source: &str) -> std::io::Result<()> {
    let width = map_data.width;
    let height = map_data.height;
    
    let water_gid = *gid_map.get("terrain_water").unwrap_or(&1);
    let mut data = vec![0; (width * height) as usize]; 

    let grass_gid = *gid_map.get("terrain_grass").unwrap_or(&1);
    let sand_gid  = *gid_map.get("terrain_sand").unwrap_or(&1);
    let rock_gid  = *gid_map.get("terrain_rock").unwrap_or(&grass_gid);
    let mountain_gid = *gid_map.get("terrain_mountain").unwrap_or(&rock_gid);
    let city_gid = *gid_map.get("terrain_city").unwrap_or(&rock_gid);
    let unknown_gid = *gid_map.get("terrain_unknown").unwrap_or(&water_gid);

    for tile in &map_data.tiles {
        let index = (tile.y as u32 * width + tile.x as u32) as usize;
        if index < data.len() {
            let gid = match tile.tile_type.as_str() {
                "Grass" => grass_gid,
                "Water" => water_gid,
                "Sand" | "SandyDesert" | "Desert"  => sand_gid,
                "Rock" => rock_gid,
                "Mountain" => mountain_gid,
                "City" | "Urban" | "Rubble" => city_gid,
                "unknown" => unknown_gid,
                _ => {
                    // println!("Warning: Unknown tile type '{}'", tile.tile_type);
                    unknown_gid
                },
            };
            data[index] = gid;
        }
    }
    
    // ... (rest of generate_tmj same as before up to location loop) ...
    let tilesets = vec![
        TilesetRef { firstgid: 1, source: tileset_source.to_string() }
    ];

    let tile_layer = Layer::TileLayer {
        id: 1,
        name: "Terrain".to_string(),
        data,
        height,
        width,
        opacity: 1.0,
        visible: true,
        x: 0,
        y: 0,
    };

    let mut icon_objects = Vec::new();
    let mut label_objects = Vec::new();
    let mut next_obj_id = 1;

    for loc in &map_data.locations {
        let icon_gid = gid_map.get(&loc.name)
            .or_else(|| gid_map.get(loc.icon_type.to_string()))
            .or_else(|| gid_map.get("marker"))
            .cloned();

        // Filter by type if a filter list is provided
        if let Some(types) = allowed_types {
            let type_str = loc.icon_type.to_string();
            if !types.iter().any(|&t| t.eq_ignore_ascii_case(&type_str)) {
                continue;
            }
        }

        // Apply visual offset for Fake Vaults (purely cosmetic, doesn't affect rubbersheeting)
        let mut adjusted_x = loc.x;
        let mut adjusted_y = loc.y;
        
        // Pixel coordinates (Approx for Stagger Y, H=76, S=38)
        // Vertical Step = 19 * 3 = 57. Horizontal Step = 66 (odd row center offset 33).
        // For smooth placement, we interpolate the 33px stagger based on Y.
        // Tiled's stagger is usually discrete, but for high-precision GPS markers,
        // we use the fractional Y to smoothly offset X.
        let stagger_offset = if (adjusted_y.floor() as i32) % 2 == 0 {
            // Even row -> moving towards odd row
            (adjusted_y - adjusted_y.floor()) * 33.0
        } else {
            // Odd row -> moving towards even row
            33.0 - (adjusted_y - adjusted_y.floor()) * 33.0
        };

        let icon_x = adjusted_x * 66.0 + stagger_offset;
        let icon_y = adjusted_y * 57.0;

        if let Some(gid) = icon_gid {
            // Center the icon on the hex
            // Hex center is roughly at (x + 33, y + 38) in pixel coords relative to top-left
            // But Tiled positions objects based on their bottom-left origin if they have a GID? 
            // Actually Tiled alignment varies. Let's assume standard object placement.
            
            let icon_w = 48.0; // Smaller icon
            let icon_h = 48.0;
            
            // Center alignment adjustment
            let offset_x = (66.0 - icon_w) / 2.0;
            let offset_y = (76.0 - icon_h) / 2.0;

            icon_objects.push(Object {
                id: next_obj_id,
                name: loc.name.clone(),
                obj_type: loc.icon_type.to_string().to_string(), 
                width: icon_w,
                height: icon_h,
                rotation: 0.0,
                gid: Some(gid),
                visible: true,
                x: icon_x + offset_x,
                y: icon_y + offset_y + icon_h, // Tiled GID objects align bottom-left, shift down
                // Wait, standard objects (rects) are top-left. GID objects are bottom-left. 
                // Let's try standardizing.
                text: None,
                point: Some(false),
            });
            next_obj_id += 1;
        }

        // Add Label Text Object (Floating above)
        label_objects.push(Object {
            id: next_obj_id,
            name: format!("{}_label", loc.name),
            obj_type: "Text".to_string(),
            width: 500.0, // Wider text box to fit long names
            height: 20.0,
            rotation: 0.0,
            gid: None, 
            visible: true,
            x: icon_x - 217.0, // Center relative to hex (66/2 - 500/2 = -217)
            y: icon_y - 5.0, // Move closer to icon (was -25.0)
            point: Some(false),
            text: Some(Text {
                text: loc.name.clone(),
                color: "#FFFFFF".to_string(), 
                bold: Some(true),
                halign: "center".to_string(),
                valign: "bottom".to_string(),
                pixelsize: 14.0, 
            }),
        });
        next_obj_id += 1;
    }

    let object_layer = Layer::ObjectGroup {
        id: 2,
        name: "Locations".to_string(),
        objects: icon_objects,
        opacity: 1.0,
        visible: true,
        x: 0,
        y: 0,
    };

    let text_layer = Layer::ObjectGroup {
        id: 3,
        name: "Labels".to_string(),
        objects: label_objects,
        opacity: 1.0,
        visible: true,
        x: 0,
        y: 0,
    };

    let map = TiledMap {
        compressionlevel: -1,
        height,
        width,
        infinite: false,
        layers: vec![tile_layer, object_layer, text_layer],
        nextlayerid: 4,
        nextobjectid: next_obj_id,
        orientation: "hexagonal".to_string(),
        renderorder: "right-down".to_string(),
        tiledversion: "1.10.2".to_string(),
        tileheight: 76,
        tilesets,
        tilewidth: 66,
        map_type: "map".to_string(),
        version: "1.10".to_string(),
        hexsidelength: Some(38), 
        staggeraxis: Some("y".to_string()),
        staggerindex: Some("odd".to_string()),
    };

    let json = serde_json::to_string_pretty(&map)?;
    fs::write(output_dir.join(filename), json)?;
    println!("Generated {} ({}x{})", filename, width, height);

    Ok(())
}

use crate::schema::RawMapData;
pub fn generate_raw_tmj(output_dir: &Path, raw_data: &RawMapData, gid_map: &HashMap<String, u32>) -> std::io::Result<()> {
    // Standard 1x Resolution
    let width = raw_data.width;
    let height = raw_data.height;
    
    let water_gid = *gid_map.get("terrain_water").unwrap_or(&1);
    let grass_gid = *gid_map.get("terrain_grass").unwrap_or(&1);
    let sand_gid  = *gid_map.get("terrain_sand").unwrap_or(&1);
    let rock_gid  = *gid_map.get("terrain_rock").unwrap_or(&grass_gid);
    let mountain_gid = *gid_map.get("terrain_mountain").unwrap_or(&rock_gid);
    let city_gid = *gid_map.get("terrain_city").unwrap_or(&rock_gid);
    let unknown_gid = *gid_map.get("terrain_unknown").unwrap_or(&water_gid);

    // Initialize with Sand (or Unknown/Void if we prefer)
    let mut data = vec![sand_gid; (width * height) as usize]; 

    // 1. Render Base Tiles (if present)
    if !raw_data.tiles.is_empty() {
        for tile in &raw_data.tiles {
            let gid = match tile.tile_type.as_str() {
                "Grass" => grass_gid,
                "Water" => water_gid,
                "Sand" => sand_gid,
                "Rock" => rock_gid,
                "Mountain" => mountain_gid,
                "City" => city_gid,
                _ => unknown_gid,
            };
            
            // Render 1x1 Tile
            if tile.x >= 0 && tile.x < width as i32 && tile.y >= 0 && tile.y < height as i32 {
                 let idx = (tile.y as u32 * width + tile.x as u32) as usize;
                 data[idx] = gid;
            }
        }
    }

    // 2. Render Detail Tiles (REMOVED)
    // Legacy support removed.


    let tilesets = vec![
        TilesetRef { firstgid: 1, source: "generic.tsj".to_string() }
    ];

    let tile_layer = Layer::TileLayer {
        id: 1,
        name: "Raw Extracted".to_string(),
        data,
        height,
        width,
        opacity: 1.0,
        visible: true,
        x: 0,
        y: 0,
    };
    
    // 3. Render Locations (if present)
    let mut objects = Vec::new();
    let mut next_obj_id = 1;
    let marker_gid = *gid_map.get("marker").unwrap_or(&1);
    
    // Scale factor based on resolution
    let loc_scale = 1.0;
    
    /*
    if !raw_data.locations.is_empty() {
        for loc in &raw_data.locations {
            let grid_x = (loc.x / 50.0) * loc_scale;
            let grid_y = (loc.y / 50.0) * loc_scale;
            
            objects.push(Object {
                id: next_obj_id,
                name: loc.name.clone(),
                obj_type: "Location".to_string(),
                width: 40.0, 
                height: 40.0,
                rotation: 0.0,
                gid: Some(marker_gid), 
                visible: true,
                x: grid_x * 66.0, 
                y: grid_y * 76.0, 
                point: Some(false),
                text: Some(Text {
                    text: loc.name.clone(),
                    color: "#FFFFFF".to_string(), 
                    bold: Some(true),
                    halign: "center".to_string(),
                    valign: "bottom".to_string(),
                    pixelsize: 12.0,
                }),
            });
            next_obj_id += 1;
        }
    }
    */

    // Object layer
    let object_layer = Layer::ObjectGroup {
        id: 2,
        name: "Locations".to_string(),
        objects,
        opacity: 1.0,
        visible: true,
        x: 0,
        y: 0,
    };

    let map = TiledMap {
        compressionlevel: -1,
        height,
        width,
        infinite: false,
        layers: vec![tile_layer, object_layer],
        nextlayerid: 3,
        nextobjectid: next_obj_id,
        orientation: "hexagonal".to_string(),
        renderorder: "right-down".to_string(),
        tiledversion: "1.10.2".to_string(),
        tileheight: 76,
        tilesets,
        tilewidth: 66,
        map_type: "map".to_string(),
        version: "1.10".to_string(),
        hexsidelength: Some(38), 
        staggeraxis: Some("y".to_string()),
        staggerindex: Some("odd".to_string()),
    };

    let json = serde_json::to_string_pretty(&map)?;
    fs::write(output_dir.join("raw_extracted.tmj"), json)?;
    println!("Generated raw_extracted.tmj ({}x{})", width, height);

    Ok(())
}
