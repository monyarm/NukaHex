use image::{Rgba, RgbaImage};
use imageproc::drawing::draw_polygon_mut;
use imageproc::point::Point;
use std::path::Path;
use std::fs;

pub fn generate_tiles(output_dir: &Path) -> std::io::Result<()> {
    if !output_dir.exists() {
        fs::create_dir_all(output_dir)?;
    }

    let tiles = vec![
        ("grass.png", Rgba([34, 139, 34, 255]), Rgba([20, 80, 20, 255])),
        ("water.png", Rgba([30, 144, 255, 255]), Rgba([10, 80, 180, 255])),
        ("sand.png", Rgba([238, 214, 175, 255]), Rgba([180, 160, 120, 255])),
        ("rock.png", Rgba([128, 128, 128, 255]), Rgba([80, 80, 80, 255])),
        ("mountain.png", Rgba([139, 115, 85, 255]), Rgba([100, 80, 60, 255])),  // Brown
        ("city.png", Rgba([128, 128, 128, 255]), Rgba([90, 90, 90, 255])),     // Grey
        ("unknown.png", Rgba([128, 0, 128, 255]), Rgba([80, 0, 80, 255])),     // Purple
    ];

    for (filename, fill, border) in tiles {
        let path = output_dir.join(filename);
        if path.exists() {
            println!("Skipping existing tile: {}", filename);
            continue;
        }

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

        img.save(&path).expect("Failed to save hex tile");
        println!("Generated hex tile: {} ({}x{})", filename, w, h);
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
