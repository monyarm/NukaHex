//! In-process PNG rendering of our Tiled maps (replaces the external `tmxrasterizer`).
//! Renders straight from the written `.tmj` and its `.tsj`, so the PNG always matches the TMJ.
//! Supports what `map_gen` emits: hexagonal maps staggered on y (odd rows shifted), one
//! image-collection tileset, tile objects (icons) and text objects (labels).

use crate::tiled_structs::{Layer, Object, TiledMap, Tileset};
use image::{imageops, Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};
use log::warn;
use rusttype::{Font, Scale};
use std::collections::HashMap;
use std::io;
use std::path::Path;

const FONT_PATH: &str = "input/fonts/DejaVuSans-Bold.ttf";
const GID_MASK: u32 = 0x1FFF_FFFF; // strip Tiled's flip flags

lazy_static::lazy_static! {
    static ref FONT: Option<Font<'static>> = {
        let font = std::fs::read(FONT_PATH).ok().and_then(Font::try_from_vec);
        if font.is_none() { warn!("[Render] {} missing: labels are not drawn", FONT_PATH); }
        font
    };
}

fn invalid(msg: String) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, msg) }

/// Renders `tmj` to `png`.
pub fn render_tmj(tmj: &Path, png: &Path) -> io::Result<()> {
    let map: TiledMap = serde_json::from_slice(&std::fs::read(tmj)?).map_err(|e| invalid(format!("{}: {}", tmj.display(), e)))?;
    let base = tmj.parent().unwrap_or(Path::new("."));

    // gid -> tile image (image-collection tilesets).
    let mut images: HashMap<u32, RgbaImage> = HashMap::new();
    for ts_ref in &map.tilesets {
        let ts_path = base.join(&ts_ref.source);
        let ts: Tileset = serde_json::from_slice(&std::fs::read(&ts_path)?).map_err(|e| invalid(format!("{}: {}", ts_path.display(), e)))?;
        let ts_dir = ts_path.parent().unwrap_or(Path::new("."));
        for tile in ts.tiles.iter().flatten() {
            match image::open(ts_dir.join(&tile.image)) {
                Ok(img) => { images.insert(ts_ref.firstgid + tile.id, img.to_rgba8()); }
                Err(e) => warn!("[Render] tile image {}: {}", tile.image, e),
            }
        }
    }

    // Hex geometry, staggered on y: odd rows shift right by half a tile.
    let (tw, th) = (map.tilewidth as i64, map.tileheight as i64);
    let side = map.hexsidelength.unwrap_or(0) as i64;
    let row_step = (th + side) / 2;
    let odd_shift = |y: i64| if map.staggerindex.as_deref() == Some("even") { (y % 2 == 0) as i64 } else { (y % 2 != 0) as i64 } * tw / 2;
    let (w, h) = (map.width as i64, map.height as i64);
    let mut canvas = RgbaImage::new((w * tw + tw / 2) as u32, (h * row_step + (th - row_step)) as u32);

    let mut scaled: HashMap<(u32, u32, u32), RgbaImage> = HashMap::new();
    for layer in &map.layers {
        match layer {
            Layer::TileLayer { data, width, visible: true, .. } => {
                for (i, &raw) in data.iter().enumerate() {
                    let Some(img) = images.get(&(raw & GID_MASK)) else { continue };
                    let (x, y) = ((i % *width as usize) as i64, (i / *width as usize) as i64);
                    // Tiled anchors tile images at the cell's bottom-left.
                    let (px, py) = (x * tw + odd_shift(y), y * row_step + th - img.height() as i64);
                    imageops::overlay(&mut canvas, img, px, py);
                }
            }
            Layer::ObjectGroup { objects, visible: true, .. } => {
                for obj in objects { draw_object(&mut canvas, obj, &images, &mut scaled); }
            }
            _ => {}
        }
    }
    canvas.save(png).map_err(|e| invalid(format!("{}: {}", png.display(), e)))
}

fn draw_object(canvas: &mut RgbaImage, obj: &Object, images: &HashMap<u32, RgbaImage>, scaled: &mut HashMap<(u32, u32, u32), RgbaImage>) {
    if let Some(points) = &obj.polyline {
        // Borders: black, wider than the 2 px hex outlines; country borders wider still.
        let (width, colour) = match obj.obj_type.as_str() {
            "CountryBorder" => (9.0, Rgba([25, 25, 25, 255])),
            _ => (6.0, Rgba([25, 25, 25, 255])),
        };
        for pair in points.windows(2) {
            let (a, b) = ((obj.x + pair[0].x, obj.y + pair[0].y), (obj.x + pair[1].x, obj.y + pair[1].y));
            draw_thick_segment(canvas, a, b, width, colour);
        }
    }
    if let Some(gid) = obj.gid {
        // Tile objects: (x, y) is the bottom-left corner; the image is stretched to the object size.
        let gid = gid & GID_MASK;
        let (ow, oh) = (obj.width.round().max(1.0) as u32, obj.height.round().max(1.0) as u32);
        if let Some(src) = images.get(&gid) {
            let img = scaled.entry((gid, ow, oh))
                .or_insert_with(|| imageops::resize(src, ow, oh, imageops::FilterType::Triangle));
            imageops::overlay(canvas, img, obj.x.round() as i64, (obj.y - obj.height).round() as i64);
        }
    }
    if let (Some(text), Some(font)) = (&obj.text, FONT.as_ref()) {
        let scale = Scale::uniform(text.pixelsize);
        let (tw, th) = text_size(scale, font, &text.text);
        let x = obj.x + match text.halign.as_str() { "center" => (obj.width - tw as f32) / 2.0, "right" => obj.width - tw as f32, _ => 0.0 };
        let y = obj.y + match text.valign.as_str() { "center" => (obj.height - th as f32) / 2.0, "bottom" => obj.height - th as f32, _ => 0.0 };
        draw_text_mut(canvas, parse_color(&text.color), x.round() as i32, y.round() as i32, scale, font, &text.text);
    }
}

/// A segment `width` px wide with round joins (a disc at each end), so chained segments meet cleanly.
fn draw_thick_segment(canvas: &mut RgbaImage, a: (f32, f32), b: (f32, f32), width: f32, colour: Rgba<u8>) {
    use imageproc::drawing::{draw_filled_circle_mut, draw_polygon_mut};
    use imageproc::point::Point;
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
    let quad = [(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)]
        .map(|(x, y)| Point::new(x.round() as i32, y.round() as i32));
    if quad.windows(2).any(|w| w[0] != w[1]) && quad[0] != quad[3] { draw_polygon_mut(canvas, &quad, colour); }
    for p in [a, b] { draw_filled_circle_mut(canvas, (p.0.round() as i32, p.1.round() as i32), (width / 2.0) as i32, colour); }
}

/// "#RRGGBB" or "#AARRGGBB" (Tiled's format); white on parse failure.
fn parse_color(s: &str) -> Rgba<u8> {
    let hex = s.trim_start_matches('#');
    let v = u32::from_str_radix(hex, 16).unwrap_or(0xFFFFFF);
    let [a, r, g, b] = if hex.len() == 8 { v.to_be_bytes() } else { [255, (v >> 16) as u8, (v >> 8) as u8, v as u8] };
    Rgba([r, g, b, a])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiled_structs::{TileDef, TilesetRef};

    /// A 3x3 hex map with one solid-colour tile per gid: each hex centre has its tile's colour,
    /// and odd rows are shifted half a tile right.
    #[test]
    fn hex_tiles_land_in_their_cells() {
        let dir = std::env::temp_dir().join(format!("render_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let colours = [[255u8, 0, 0], [0, 255, 0], [0, 0, 255]];
        let mut tiles = Vec::new();
        for (i, c) in colours.iter().enumerate() {
            let name = format!("t{}.png", i);
            RgbaImage::from_pixel(66, 76, Rgba([c[0], c[1], c[2], 255])).save(dir.join(&name)).unwrap();
            tiles.push(TileDef { id: i as u32, image: name, imageheight: 76, imagewidth: 66 });
        }
        let ts = Tileset { columns: 0, image: None, imageheight: None, imagewidth: None, margin: 0, name: "t".into(), spacing: 0,
            tilecount: 3, tileheight: 76, tilewidth: 66, tileset_type: "tileset".into(), version: "1.10".into(),
            tiledversion: "1.10.2".into(), tiles: Some(tiles), tileoffset: None };
        std::fs::write(dir.join("t.tsj"), serde_json::to_vec(&ts).unwrap()).unwrap();
        // gid per cell: row y uses colour y (gid = y + 1).
        let data: Vec<u32> = (0..9).map(|i| (i / 3) as u32 + 1).collect();
        let map = TiledMap { compressionlevel: -1, height: 3, width: 3, infinite: false, nextlayerid: 2, nextobjectid: 1,
            layers: vec![Layer::TileLayer { id: 1, name: "T".into(), data, height: 3, width: 3, opacity: 1.0, visible: true, x: 0, y: 0 }],
            orientation: "hexagonal".into(), renderorder: "right-down".into(), tiledversion: "1.10.2".into(), tileheight: 76,
            tilesets: vec![TilesetRef { firstgid: 1, source: "t.tsj".into() }], tilewidth: 66, hexsidelength: Some(38),
            staggeraxis: Some("y".into()), staggerindex: Some("odd".into()), map_type: "map".into(), version: "1.10".into() };
        std::fs::write(dir.join("m.tmj"), serde_json::to_vec(&map).unwrap()).unwrap();

        render_tmj(&dir.join("m.tmj"), &dir.join("m.png")).unwrap();
        let img = image::open(dir.join("m.png")).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (3 * 66 + 33, 3 * 57 + 19));
        for y in 0..3u32 { for x in 0..3u32 {
            let (cx, cy) = (x * 66 + 33 + if y % 2 == 1 { 33 } else { 0 }, y * 57 + 38);
            let p = img.get_pixel(cx, cy).0;
            assert_eq!(&p[..3], &colours[y as usize], "cell ({}, {})", x, y);
        }}
        // Odd row 1 starts 33 px in: its left edge is empty.
        assert_eq!(img.get_pixel(5, 57 + 38).0[3], 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
