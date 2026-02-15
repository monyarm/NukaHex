use crate::tiled_structs::*;
use crate::schema::FixedMapData;
use std::fs;
use std::path::{Path, PathBuf};
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
                             next_id += 1;
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
             next_id += 1;
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

pub fn generate_tmj(output_dir: &Path, map_data: &FixedMapData, gid_map: &HashMap<String, u32>) -> std::io::Result<()> {
    let width = map_data.width;
    let height = map_data.height;
    
    let water_gid = *gid_map.get("terrain_water").unwrap_or(&1);
    let mut data = vec![water_gid; (width * height) as usize]; 

    let grass_gid = *gid_map.get("terrain_grass").unwrap_or(&1);
    let sand_gid  = *gid_map.get("terrain_sand").unwrap_or(&1);
    let rock_gid  = *gid_map.get("terrain_rock").unwrap_or(&grass_gid);
    let mountain_gid = *gid_map.get("terrain_mountain").unwrap_or(&rock_gid);
    let city_gid = *gid_map.get("terrain_city").unwrap_or(&rock_gid);
    let unknown_gid = *gid_map.get("terrain_unknown").unwrap_or(&water_gid);

    for tile in &map_data.tiles {
        let index = (tile.y * width + tile.x) as usize;
        if index < data.len() {
            let gid = match tile.tile_type.as_str() {
                "Grass" => grass_gid,
                "Water" => water_gid,
                "Sand" => sand_gid,
                "Rock" => rock_gid,
                "Mountain" => mountain_gid,
                "City" => city_gid,
                "unknown" => unknown_gid,
                _ => water_gid,
            };
            data[index] = gid;
        }
    }
    
    // ... (rest of generate_tmj same as before up to location loop) ...
    let tilesets = vec![
        TilesetRef { firstgid: 1, source: "generic.tsj".to_string() }
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

        
        // Pixel coordinates (Approx for Stagger Y, H=76, S=38)
        // Vertical Step = 57. Horizontal Step = 66 (odd row offset 33).
        let icon_x = loc.x * 66.0f32;
        let icon_y = loc.y * 57.0f32; 

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
            width: 200.0, // Wider text box to fit long names
            height: 20.0,
            rotation: 0.0,
            gid: None, 
            visible: true,
            x: icon_x - 67.0, // Center relative to hex (66/2 - 200/2 = -67)
            y: icon_y - 15.0, // Move closer to icon (was -25.0)
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
    fs::write(output_dir.join("map.tmj"), json)?;
    println!("Generated map.tmj ({}x{})", width, height);

    Ok(())
}

use crate::schema::RawMapData;
pub fn generate_raw_tmj(output_dir: &Path, raw_data: &RawMapData, gid_map: &HashMap<String, u32>) -> std::io::Result<()> {
    // Determine bounds
    let max_x = raw_data.tiles.iter().map(|t| t.x).max().unwrap_or(0);
    let max_y = raw_data.tiles.iter().map(|t| t.y).max().unwrap_or(0);
    let width = max_x + 1;
    let height = max_y + 1;
    
    let water_gid = *gid_map.get("terrain_water").unwrap_or(&1);
    let grass_gid = *gid_map.get("terrain_grass").unwrap_or(&1);
    let sand_gid  = *gid_map.get("terrain_sand").unwrap_or(&1);
    let rock_gid  = *gid_map.get("terrain_rock").unwrap_or(&grass_gid);
    let mountain_gid = *gid_map.get("terrain_mountain").unwrap_or(&rock_gid);
    let city_gid = *gid_map.get("terrain_city").unwrap_or(&rock_gid);

    let mut data = vec![sand_gid; (width * height) as usize]; // Default to sand for extracted data? Or water.

    for tile in &raw_data.tiles {
        let index = (tile.y * width + tile.x) as usize;
        if index < data.len() {
            let gid = match tile.tile_type.as_str() {
                "Grass" => grass_gid,
                "Water" => water_gid,
                "Sand" => sand_gid,
                "Rock" => rock_gid,
                "Mountain" => mountain_gid,
                "City" => city_gid,
                _ => water_gid,
            };
            data[index] = gid;
        }
    }

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
    
    // Also render locations as objects? simple dots
    let mut objects = Vec::new();
    let mut next_obj_id = 1;
    let marker_gid = *gid_map.get("marker").unwrap_or(&1);
    
    for loc in &raw_data.locations {
         objects.push(Object {
            id: next_obj_id,
           name: loc.name.clone(),
            obj_type: "Location".to_string(),
            width: 66.0, 
            height: 76.0,
            rotation: 0.0,
            gid: Some(marker_gid), 
            visible: true,
            x: loc.x * 10.0, // Scale down locations? Raw locs are 0-1400. Grid is 0-28. 
            // Warning: Raw locations use PIXEL coords, Raw tiles use GRID coords. 
            // They won't align without the 50x scale. 
            // Let's just plot them raw.
            y: loc.y * 10.0, 
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

    let map = TiledMap {
        compressionlevel: -1,
        height,
        width,
        infinite: false,
        layers: vec![tile_layer], // Just tiles for now to verify data
        nextlayerid: 2,
        nextobjectid: 1,
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
