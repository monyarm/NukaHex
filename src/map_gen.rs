use crate::tiled_structs::*;
use crate::schema::FixedMapData;
use std::fs;
use std::path::Path;
use std::collections::HashMap;
use serde_json;
use log::{info, warn};

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

    for (name, ..) in crate::tile_gen::TERRAINS {
        let filename = &format!("{}.png", name.to_lowercase());
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
    info!("Generated tileset generic.tsj");
    Ok(gid_map)
}

/// Gid of a tile type's terrain tile (aliases resolved); unknown types use the "unknown" tile.
fn terrain_gid(gid_map: &HashMap<String, u32>, tile_type: &str) -> u32 {
    let key = |name: &str| format!("terrain_{}", name.to_lowercase());
    gid_map.get(&key(crate::tile_gen::canonical_terrain(tile_type)))
        .or_else(|| gid_map.get(&key("unknown")))
        .copied().unwrap_or(1)
}

/// Top-left pixel of the hex cell at fractional grid position (x, y). Odd rows are shifted half a
/// hex; the shift is interpolated by the fractional row so continuous positions move smoothly.
fn hex_px(x: f32, y: f32) -> (f32, f32) {
    // Grid positions already carry the stagger (odd-row hex centres sit at col + 0.5; see
    // `fixer::hex_center`), so pixels are a plain scale: 66 px per column, 57 px per row.
    (x * 66.0, y * 57.0)
}

/// Border lines between tiles of different real-world regions, following hex edges (so straight
/// borders zigzag along the grid). Only drawn between two tiles that both have a region, so
/// borders never extend past mapped land. One polyline object per connected run of edges, typed
/// "CountryBorder" or "StateBorder".
fn border_layer(tiles: &[crate::schema::FixedTile], layer_id: u32, next_obj_id: &mut u32) -> Layer {
    use std::collections::HashMap as Map;
    let neighbours = crate::fixer::hex_neighbours;
    let mut region: Map<(i32, i32), u16> = tiles.iter().filter_map(|t| t.region.map(|r| ((t.x, t.y), r))).collect();
    // Land tiles outside every real-world region (the warped game coast reaches a little past the
    // real one) take the region of the nearest tile that has one, so borders run to the coast.
    let land: std::collections::HashSet<(i32, i32)> = tiles.iter()
        .filter(|t| t.tile_type != "unknown" && t.tile_type != "Water").map(|t| (t.x, t.y)).collect();
    let mut frontier: Vec<(i32, i32)> = region.keys().copied().collect();
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for (x, y) in frontier {
            let r = region[&(x, y)];
            for n in neighbours(x, y) {
                if land.contains(&n) && !region.contains_key(&n) { region.insert(n, r); next.push(n); }
            }
        }
        frontier = next;
    }
    // Segments in pixels, by kind. Tiles are 66x76, odd rows shifted right by 33, rows 57 apart.
    let mut segments: Map<bool, Vec<((i32, i32), (i32, i32))>> = Map::new();
    for (&(x, y), &r) in &region {
        let odd = y.rem_euclid(2) == 1;
        let (ox, oy) = (x * 66 + if odd { 33 } else { 0 }, y * 57);
        let (se, sw) = if odd { ((x + 1, y + 1), (x, y + 1)) } else { ((x, y + 1), (x - 1, y + 1)) };
        let edges = [((x + 1, y), (ox + 66, oy + 19), (ox + 66, oy + 57)),
                     (se, (ox + 66, oy + 57), (ox + 33, oy + 76)),
                     (sw, (ox + 33, oy + 76), (ox, oy + 57))];
        for (n, a, b) in edges {
            if let Some(&rn) = region.get(&n) {
                if rn != r { segments.entry(crate::borders::is_country_border(r, rn)).or_default().push((a, b)); }
            }
        }
    }
    // Chain segments sharing endpoints into polylines.
    let mut objects = Vec::new();
    for (country, segs) in segments {
        let mut at: Map<(i32, i32), Vec<usize>> = Map::new();
        for (i, (a, b)) in segs.iter().enumerate() { at.entry(*a).or_default().push(i); at.entry(*b).or_default().push(i); }
        let mut used = vec![false; segs.len()];
        for start in 0..segs.len() {
            if used[start] { continue; }
            used[start] = true;
            let mut line = vec![segs[start].0, segs[start].1];
            for forward in [true, false] {
                loop {
                    let end = if forward { *line.last().unwrap() } else { line[0] };
                    let Some(&i) = at[&end].iter().find(|&&i| !used[i]) else { break };
                    used[i] = true;
                    let next = if segs[i].0 == end { segs[i].1 } else { segs[i].0 };
                    if forward { line.push(next) } else { line.insert(0, next) }
                }
            }
            let (x0, y0) = line[0];
            objects.push(Object {
                id: *next_obj_id, name: String::new(),
                obj_type: if country { "CountryBorder" } else { "StateBorder" }.into(),
                width: 0.0, height: 0.0, rotation: 0.0, gid: None, visible: true,
                x: x0 as f32, y: y0 as f32, point: None, text: None,
                polyline: Some(line.iter().map(|&(x, y)| PolyPoint { x: (x - x0) as f32, y: (y - y0) as f32 }).collect()),
            });
            *next_obj_id += 1;
        }
    }
    Layer::ObjectGroup { id: layer_id, name: "Borders".into(), objects, opacity: 1.0, visible: true, x: 0, y: 0 }
}

pub fn generate_tmj(output_dir: &Path, filename: &str, map_data: &FixedMapData, gid_map: &HashMap<String, u32>, allowed_types: Option<&[&str]>, tileset_source: &str) -> std::io::Result<()> {
    let width = map_data.width;
    let height = map_data.height;

    if width > 500 || height > 500 {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Map size too large: {}x{}", width, height)));
    }
    
    let mut data = vec![0; (width * height) as usize]; 

    for tile in &map_data.tiles {
        let index = (tile.y as u32 * width + tile.x as u32) as usize;
        if index < data.len() {
            let gid = terrain_gid(gid_map, &tile.tile_type);
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

        if loc.name == "Fake Vault 13a" {
            adjusted_x += 0.5;
            adjusted_y += 0.5;
        }
        
        // Pixel position of the marker's grid position (its hex's top-left when on a hex centre).
        let (icon_x, icon_y) = hex_px(adjusted_x, adjusted_y);

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
                polyline: None,
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
            polyline: None,
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
        layers: vec![tile_layer, border_layer(&map_data.tiles, 4, &mut next_obj_id), object_layer, text_layer],
        nextlayerid: 5,
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
    info!("Generated {} ({}x{})", filename, width, height);

    Ok(())
}

use crate::schema::RawMapData;
pub fn generate_raw_tmj(output_dir: &Path, filename: &str, raw_data: &RawMapData, gid_map: &HashMap<String, u32>, tileset_source: &str) -> std::io::Result<()> {
    // Standard 1x Resolution
    let width = raw_data.width;
    let height = raw_data.height;

    if width > 10000 || height > 10000 {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Raw map size too large: {}x{}", width, height)));
    }
    
    // Initialize with Sand (or Unknown/Void if we prefer)
    let mut data = vec![terrain_gid(gid_map, "Sand"); (width * height) as usize]; 

    // 1. Render Base Tiles (if present)
    if !raw_data.tiles.is_empty() {
        for tile in &raw_data.tiles {
            let gid = terrain_gid(gid_map, &tile.tile_type);
            
            // Render 1x1 Tile
            if tile.x >= 0 && tile.x < width as i32 && tile.y >= 0 && tile.y < height as i32 {
                 let idx = (tile.y as u32 * width + tile.x as u32) as usize;
                 data[idx] = gid;
            }
        }
    }


    let tilesets = vec![
        TilesetRef { firstgid: 1, source: tileset_source.to_string() }
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
    let objects = Vec::new();
    let next_obj_id = 1;
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
    fs::write(output_dir.join(filename), json)?;
    info!("Generated {} ({}x{})", filename, width, height);

    Ok(())
}
