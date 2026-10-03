//! Real-world administrative regions for drawing borders: US states, plus Canada and Mexico as
//! whole countries (Natural Earth 1:50m, public domain; downloaded once into `.cache/borders`).

use log::{info, warn};
use std::path::Path;

const BASE_URL: &str = "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson";
const STATES: &str = "ne_50m_admin_1_states_provinces_lakes.geojson";
const COUNTRIES: &str = "ne_50m_admin_0_countries_lakes.geojson";

pub struct Region {
    pub name: String,
    pub country: String,
    /// Polygon rings as (lon, lat); point-in-region uses the even-odd rule over all rings.
    rings: Vec<Vec<(f32, f32)>>,
    bbox: (f32, f32, f32, f32), // min lon, min lat, max lon, max lat
}

lazy_static::lazy_static! {
    pub static ref REGIONS: Vec<Region> = load().unwrap_or_else(|e| {
        warn!("[Borders] unavailable, no borders drawn: {}", e);
        Vec::new()
    });
}

fn fetch(name: &str) -> Result<serde_json::Value, String> {
    let path = Path::new(".cache/borders").join(name);
    if !path.exists() {
        let url = format!("{}/{}", BASE_URL, name);
        info!("[Downloader] fetching {}...", url);
        let body = ureq::get(&url).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&path, body).map_err(|e| e.to_string())?;
    }
    serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn rings_of(geometry: &serde_json::Value) -> Vec<Vec<(f32, f32)>> {
    let ring = |r: &serde_json::Value| -> Vec<(f32, f32)> {
        r.as_array().into_iter().flatten()
            .filter_map(|p| Some((p.get(0)?.as_f64()? as f32, p.get(1)?.as_f64()? as f32))).collect()
    };
    let coords = &geometry["coordinates"];
    match geometry["type"].as_str() {
        Some("Polygon") => coords.as_array().into_iter().flatten().map(ring).collect(),
        Some("MultiPolygon") => coords.as_array().into_iter().flatten()
            .flat_map(|poly| poly.as_array().into_iter().flatten().map(ring)).collect(),
        _ => Vec::new(),
    }
}

fn region(name: &str, country: &str, geometry: &serde_json::Value) -> Region {
    let rings = rings_of(geometry);
    let pts = rings.iter().flatten();
    let bbox = pts.fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, &(x, y)| (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y)));
    Region { name: name.into(), country: country.into(), rings, bbox }
}

fn load() -> Result<Vec<Region>, String> {
    let mut out = Vec::new();
    for f in fetch(STATES)?["features"].as_array().into_iter().flatten() {
        if f["properties"]["adm0_a3"] == "USA" {
            out.push(region(f["properties"]["name"].as_str().unwrap_or("?"), "USA", &f["geometry"]));
        }
    }
    for f in fetch(COUNTRIES)?["features"].as_array().into_iter().flatten() {
        let code = f["properties"]["ADM0_A3"].as_str().unwrap_or("");
        if code == "CAN" || code == "MEX" {
            out.push(region(code, code, &f["geometry"]));
        }
    }
    info!("[Borders] loaded {} regions", out.len());
    Ok(out)
}

fn contains(r: &Region, lat: f32, lon: f32) -> bool {
    if lon < r.bbox.0 || lat < r.bbox.1 || lon > r.bbox.2 || lat > r.bbox.3 { return false; }
    let mut inside = false;
    for ring in &r.rings {
        for (i, &(x1, y1)) in ring.iter().enumerate() {
            let (x2, y2) = ring[(i + 1) % ring.len()];
            if (y1 > lat) != (y2 > lat) && lon < x1 + (lat - y1) * (x2 - x1) / (y2 - y1) { inside = !inside; }
        }
    }
    inside
}

/// Index into `REGIONS` of the region containing (lat, lon), if any.
pub fn region_at(lat: f32, lon: f32) -> Option<u16> {
    REGIONS.iter().position(|r| contains(r, lat, lon)).map(|i| i as u16)
}

/// Whether the border between two regions is a country border (else a state border).
pub fn is_country_border(a: u16, b: u16) -> bool {
    REGIONS[a as usize].country != REGIONS[b as usize].country
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_fall_in_their_regions() {
        if REGIONS.is_empty() { eprintln!("skipping: border data unavailable"); return; }
        let name = |lat, lon| region_at(lat, lon).map(|i| REGIONS[i as usize].name.clone());
        assert_eq!(name(34.05, -118.24).as_deref(), Some("California"));
        assert_eq!(name(39.74, -104.99).as_deref(), Some("Colorado"));
        assert_eq!(name(32.52, -117.04).as_deref(), Some("MEX")); // Tijuana
        assert_eq!(name(49.28, -123.12).as_deref(), Some("CAN")); // Vancouver
        assert_eq!(name(35.0, -125.0), None); // Pacific
        let (ca, nv) = (region_at(37.0, -119.0).unwrap(), region_at(39.0, -117.0).unwrap());
        assert!(!is_country_border(ca, nv));
        assert!(is_country_border(ca, region_at(32.52, -117.04).unwrap()));
    }
}
