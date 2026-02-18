use serde::Deserialize; // added for geocoding
use log::{info, debug};
use std::collections::HashMap;
use std::sync::Mutex;

use std::fs;
use std::path::Path;

const CACHE_FILE: &str = ".cache/geocode_cache.json";

lazy_static::lazy_static! {
    static ref GEOCODE_CACHE: Mutex<HashMap<String, (f32, f32)>> = Mutex::new(load_cache());
}

fn load_cache() -> HashMap<String, (f32, f32)> {
    if Path::new(CACHE_FILE).exists() {
        if let Ok(content) = fs::read_to_string(CACHE_FILE) {
            if let Ok(cache) = serde_json::from_str(&content) {
                info!("Loaded geocode cache from {}", CACHE_FILE);
                return cache;
            }
        }
    }
    HashMap::new()
}

fn save_cache(cache: &HashMap<String, (f32, f32)>) {
    if let Some(parent) = Path::new(CACHE_FILE).parent() {
        fs::create_dir_all(parent).unwrap_or_default();
    }
    if let Ok(json) = serde_json::to_string_pretty(cache) {
        let _ = fs::write(CACHE_FILE, json);
    }
}

#[derive(Debug, Deserialize)]
struct NominatimResponse {
    lat: String,
    lon: String,
}

/// Geocode a place name or address to lat/lon coordinates using Nominatim API
/// Results are cached to avoid repeated API calls
pub fn geocode(query: &str) -> Result<(f32, f32), String> {
    // Check cache first
    {
        let cache = GEOCODE_CACHE.lock().unwrap();
        if let Some(&coords) = cache.get(query) {
            debug!("Geocoding cache hit for '{}'", query);
            return Ok(coords);
        }
    }
    
    info!("Geocoding '{}' via Nominatim API...", query);
    
    // Call Nominatim API
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&limit=1",
        urlencoding::encode(query)
    );
    
    let response = ureq::get(&url)
        .set("User-Agent", "FalloutMapGenerator/1.0")
        .call()
        .map_err(|e| format!("Nominatim API error: {}", e))?;
    
    let body = response
        .into_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;
    
    let results: Vec<NominatimResponse> = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse Nominatim response: {}", e))?;
    
    if results.is_empty() {
        return Err(format!("No results found for '{}'", query));
    }
    
    let lat: f32 = results[0].lat.parse()
        .map_err(|e| format!("Invalid latitude: {}", e))?;
    let lon: f32 = results[0].lon.parse()
        .map_err(|e| format!("Invalid longitude: {}", e))?;
    
    info!("Geocoded '{}' to ({}, {})", query, lat, lon);
    
    // Cache the result
    {
        let mut cache = GEOCODE_CACHE.lock().unwrap();
        cache.insert(query.to_string(), (lat, lon));
        save_cache(&cache); // Persist immediately
    }
    
    Ok((lat, lon))
}

/// Parse a coordinate string that can be either:
/// - Two numbers: "lat,lon" -> (lat, lon)
/// - A place name/address: "Griffith Observatory" -> geocoded coordinates
pub fn parse_coords(input: &str) -> Result<(f32, f32), String> {
    let input = input.trim();
    
    // Try to parse as "lat,lon"
    if let Some((lat_str, lon_str)) = input.split_once(',') {
        if let (Ok(lat), Ok(lon)) = (lat_str.trim().parse::<f32>(), lon_str.trim().parse::<f32>()) {
            return Ok((lat, lon));
        }
    }
    
    // Otherwise, treat as a place name/address and geocode
    geocode(input)
}
