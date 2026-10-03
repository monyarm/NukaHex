use image::{Rgba, RgbaImage};
use log::info;
use imageproc::drawing::draw_polygon_mut;
use imageproc::point::Point;
use std::path::Path;
use std::fs;

/// A symbol drawn in the middle of a terrain tile.
#[derive(Clone, Copy)]
pub enum Symbol {
    /// Two stacked waves (≈).
    Waves,
    /// The radiation trefoil.
    Radiation,
}

/// Every terrain type: (canonical name, accepted aliases, fill, outline, symbol). The tile image is
/// `<name lowercased>.png`; this table drives both tile generation and the map's gid lookup,
/// so adding a terrain type is one row.
pub const TERRAINS: &[(&str, &[&str], [u8; 3], [u8; 3], Option<Symbol>)] = &[
    ("Water", &["Ocean", "Lake"], [30, 144, 255], [10, 80, 180], Some(Symbol::Waves)),
    ("Sand", &["Desert", "SandyDesert", "Plains"], [238, 214, 175], [180, 160, 120], None),
    ("Mountain", &[], [139, 115, 85], [100, 80, 60], None),
    ("City", &["Urban", "Rubble"], [128, 128, 128], [90, 90, 90], None), // ruined urban area
    ("Rock", &[], [112, 128, 144], [70, 80, 90], None),                  // bare rock
    ("Grass", &[], [34, 139, 34], [20, 80, 20], None),
    ("Forest", &[], [0, 90, 40], [0, 55, 25], None),
    ("Snow", &[], [245, 248, 252], [190, 200, 210], None),
    ("Canyon", &[], [178, 90, 50], [120, 55, 30], None),
    ("Radiation", &[], [70, 255, 60], [20, 140, 30], Some(Symbol::Radiation)), // Pip-Boy green
    ("unknown", &[], [128, 0, 128], [80, 0, 80], None),
];

/// Draws `symbol` centred on the 66x76 hex tile.
fn draw_symbol(img: &mut RgbaImage, symbol: Symbol, colour: Rgba<u8>) {
    let (cx, cy) = (33.0f32, 38.0f32);
    match symbol {
        Symbol::Waves => {
            // Two sine strokes, 2 px thick: x in [-14, 14], one period and a half.
            for row in [-5.0f32, 5.0] {
                for i in 0..=280 {
                    let x = -14.0 + i as f32 * 0.1;
                    let y = row + 3.0 * (x * std::f32::consts::PI / 9.0).sin();
                    for t in [-0.8f32, 0.0, 0.8] {
                        let (px, py) = ((cx + x).round() as i32, (cy + y + t).round() as i32);
                        if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                            img.put_pixel(px as u32, py as u32, colour);
                        }
                    }
                }
            }
        }
        Symbol::Radiation => {
            // Centre dot r<3.5; three 60-degree blades between r 6 and 17, one pointing down.
            for (x, y, p) in img.enumerate_pixels_mut() {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let r = (dx * dx + dy * dy).sqrt();
                let deg = dy.atan2(dx).to_degrees().rem_euclid(360.0); // 90 = down
                let in_blade = [90.0f32, 210.0, 330.0].iter().any(|c| { let d = (deg - c).abs(); d.min(360.0 - d) <= 30.0 });
                if r < 3.5 || ((6.0..=17.0).contains(&r) && in_blade) { *p = colour; }
            }
        }
    }
}

/// Canonical terrain name for a tile type (alias-aware, case-insensitive); "unknown" if none.
pub fn canonical_terrain(tile_type: &str) -> &'static str {
    TERRAINS.iter()
        .find(|(name, aliases, ..)| name.eq_ignore_ascii_case(tile_type) || aliases.iter().any(|a| a.eq_ignore_ascii_case(tile_type)))
        .map_or("unknown", |t| t.0)
}

pub fn generate_tiles(output_dir: &Path) -> std::io::Result<()> {
    if !output_dir.exists() {
        fs::create_dir_all(output_dir)?;
    }

    for (name, _, fill, border, symbol) in TERRAINS {
        let filename = format!("{}.png", name.to_lowercase());
        let path = output_dir.join(&filename);
        let (fill, border) = (Rgba([fill[0], fill[1], fill[2], 255]), Rgba([border[0], border[1], border[2], 255]));

        // 66x76 Canvas (Mathematical Standard)
        // Width 66 is Even, allowing exact W/2 stagger (33).
        // Center is at 33.0 (between pixels 32 and 33).
        // To handle sub-pixel center symmetrically, we use a 2px wide flat top.
        let w = 66;
        let h = 76;
        let mut img = RgbaImage::new(w, h);
        
        // 1. Outer Hex (Border Color)
        // 8 points to handle 2px tips.
        let outer_points = vec![
            Point::new(32, 0), Point::new(33, 0), // Top 2px flat
            Point::new(65, 19),
            Point::new(65, 56),
            Point::new(33, 75), Point::new(32, 75), // Bottom 2px flat
            Point::new(0, 56),
            Point::new(0, 19),
        ];

        draw_polygon_mut(&mut img, &outer_points, border);

        // 2. Inner Hex (Fill Color) - Inset 2px
        // Vertices adjusted inwards by 2px relative to edges.
        let inner_points = vec![
            Point::new(32, 2), Point::new(33, 2),
            Point::new(63, 20),
            Point::new(63, 55),
            Point::new(33, 73), Point::new(32, 73),
            Point::new(2, 55),
            Point::new(2, 20),
        ];

        draw_polygon_mut(&mut img, &inner_points, fill);

        if let Some(symbol) = symbol {
            let ink = if matches!(symbol, Symbol::Radiation) { Rgba([20, 20, 20, 255]) } else { border };
            draw_symbol(&mut img, *symbol, ink);
        }

        img.save(&path).expect("Failed to save hex tile"); // always rewritten: colours may change
        info!("Generated hex tile: {} ({}x{})", filename, w, h);
    }

    // Marker (Red Circle)
    let marker_path = output_dir.join("marker.png");
    if !marker_path.exists() {
        let mut marker = RgbaImage::new(64, 64);
         for (x, y, pixel) in marker.enumerate_pixels_mut() {
            let dx = x as f32 - 32.0;
            let dy = y as f32 - 32.0;
            let dist = (dx * dx + dy * dy).sqrt();
            
            if dist < 20.0 {
                *pixel = Rgba([255, 0, 0, 255]); 
            } else if dist < 22.0 {
                *pixel = Rgba([255, 255, 255, 255]); 
            } else {
                *pixel = Rgba([0, 0, 0, 0]); 
            }
        }
        marker.save(&marker_path).expect("Failed to save marker.png");
    }

    Ok(())
}
