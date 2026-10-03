//! Bethesda games (Fallout 3 / New Vegas), decoded with Mutagen through the C# helper
//! `tools/mutagen_dump` (built by `build.rs`). The helper loads every official master of the game
//! (base game + all DLC) and dumps one worldspace's winning records; everything below classifies them.
//!
//! Frame: one tile per landscape quadrant (half a cell, 2048 units), x east, y south, origin at the
//! north-west corner of the worldspace's landscape. Every cell with landscape is used, including the
//! out-of-bounds LOD cells: they are real terrain from the game files.

use crate::extractors::Extractor;
use crate::schema::{IconType, MapBounds, RawLocation, RawMapData, RawTile};
use log::{info, warn};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const CELL: f32 = 4096.0;
const QUAD: f32 = CELL / 2.0;
const VERTEX_SPACING: f32 = CELL / 32.0;
/// Built-up share (building footprint over ground area), averaged over a quadrant and its 8
/// neighbours, above which ground is urban: Vegas and the denser towns, not lone shacks.
const CITY_FOOTPRINT: f32 = 0.3;
/// Architecture meshes that are not buildings or ruins (City also stands for rubble, so rubble and
/// walls count): roads, street furniture, infrastructure, camps, craters and the dam.
const NOT_BUILDINGS: &[&str] = &[
    "highway", "overpass", "bridge", "sidewalk", "parkinglot", "runway", "andrewsbase", "streetdressing",
    "decal", "barrier", "barricade", "fence", "monorail", "mrsingle", "transmission", "pylon", "wires",
    "docks", "tent", "crater", "hooverdam",
];

/// Lush plant share (plant footprint over ground area), averaged over a quadrant and its 8 neighbours,
/// above which ground is Grass: Vault 22's overgrowth, not desert scrub or scattered trees.
const VEGETATION_FOOTPRINT: f32 = 0.1;
/// Desert plants, trees and dead wood, which do not make ground lush: Mojave scrub, and the conifers and
/// shade trees dotted over mountains and towns.
const NOT_LUSH: &[&str] = &[
    "shrub", "yucca", "sage", "gourd", "cactus", "agave", "withered", "dead", "stump", "burnt", "tumbleweed",
    "joshua", "branchpile", "log0", "juniper", "redced", "cedar", "evergreen", "aspen", "birch", "oak", "pine",
    "sassafras", "tree",
];

/// What a placed static's mesh contributes to the ground: City (buildings and ruins), Grass (lush
/// plants), or nothing.
fn static_terrain(model: &str) -> Option<&'static str> {
    let m = model.to_lowercase();
    if m.contains("architecture") {
        return (!NOT_BUILDINGS.iter().any(|w| m.contains(w))).then_some("City");
    }
    let plant = m.contains("landscape\\plants") || m.contains("landscape\\trees") || m.starts_with("trees\\")
        || m.ends_with(".spt") || m.contains("vault22");
    (plant && !NOT_LUSH.iter().any(|w| m.contains(w))).then_some("Grass")
}

/// Mean gradient (rise over run), averaged over a quadrant and its 8 neighbours, above which ground is
/// mountain whatever its texture (~35 degrees). New Vegas is hilly throughout; this keeps the ranges
/// (about 16% of the worldspace) and leaves the basins desert.
const MOUNTAIN_SLOPE: f32 = 0.7;
/// XCLW values at or above this mean "no water in this cell".
const NO_WATER: f32 = 1.0e30;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BethesdaGameVariant {
    Fallout3,
    FalloutNV,
}

impl BethesdaGameVariant {
    fn release(&self) -> &'static str {
        match self { Self::Fallout3 => "Fallout3", Self::FalloutNV => "FalloutNV" }
    }
    fn app_id(&self) -> u32 {
        match self { Self::Fallout3 => 22370, Self::FalloutNV => 22380 }
    }
    fn name(&self) -> &'static str {
        match self { Self::Fallout3 => "Fallout 3", Self::FalloutNV => "Fallout: New Vegas" }
    }
    fn id_str(&self) -> &'static str {
        match self { Self::Fallout3 => "fallout3", Self::FalloutNV => "falloutnv" }
    }
}

pub struct BethesdaExtractor {
    variant: BethesdaGameVariant,
    worldspace: &'static str,
    id: String,
}

impl BethesdaExtractor {
    pub fn new(variant: BethesdaGameVariant, worldspace: &'static str) -> Self {
        Self { variant, worldspace, id: format!("{}_{}", variant.id_str(), worldspace.to_lowercase()) }
    }

    fn data_dir(&self) -> Option<PathBuf> {
        let install = crate::utils::load_config_paths().ok()
            .and_then(|p| p.get(self.variant.id_str()).map(PathBuf::from))
            .or_else(|| crate::dat_extractor::find_steam_app(self.variant.app_id(), self.variant.name()).ok())?;
        Some(install.join("Data")).filter(|d| d.is_dir())
    }

    fn dump(&self, data_dir: &Path) -> Result<Dump, String> {
        let out = Path::new(".cache/bethesda").join(format!("{}.json", self.id));
        std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
        let result = Command::new(env!("MUTAGEN_DOTNET"))
            .arg(env!("MUTAGEN_DUMP_DLL"))
            .args([self.variant.release(), &data_dir.to_string_lossy(), self.worldspace, &out.to_string_lossy()])
            .output().map_err(|e| e.to_string())?;
        let log = String::from_utf8_lossy(&result.stderr);
        if !result.status.success() {
            return Err(format!("mutagen_dump failed: {}", log.trim()));
        }
        info!("[{}] {}", self.variant.name(), log.trim());
        let dump = std::fs::read(&out).map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()));
        let _ = std::fs::remove_file(&out); // the extractor's own output is what gets cached
        dump
    }
}

impl Extractor for BethesdaExtractor {
    fn get_id(&self) -> &str { &self.id }
    fn get_code_path(&self) -> &str { "src/extractors/bethesda.rs" }
    fn get_cluster_id(&self) -> String { self.id.clone() }

    /// Every master in Data (the helper picks the official ones) plus the helper's source.
    fn get_source_paths(&self) -> Vec<String> {
        let mut paths = vec!["tools/mutagen_dump/Program.cs".to_string()];
        if let Some(dir) = self.data_dir() {
            let mut esms: Vec<String> = std::fs::read_dir(dir).into_iter().flatten().flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("esm")))
                .map(|p| p.to_string_lossy().into_owned()).collect();
            esms.sort();
            paths.extend(esms);
        }
        paths
    }

    fn extract(&self) -> RawMapData {
        let failed = |msg: String| {
            warn!("[{}] {}", self.variant.name(), msg);
            RawMapData { warnings: vec![msg], coordinate_scale: QUAD, priority: 1, ..Default::default() }
        };
        let Some(data_dir) = self.data_dir() else {
            return failed(format!("{} installation not found", self.variant.name()));
        };
        match self.dump(&data_dir) {
            Ok(dump) => build(&dump),
            Err(e) => failed(e),
        }
    }
}

// --- Helper output ---

#[derive(Deserialize)]
struct Dump {
    worldspace: DumpWorld,
    markers: Vec<DumpMarker>,
    #[serde(default)]
    child_markers: Vec<DumpChildMarker>,
    radiation: Vec<DumpRadiation>,
    #[serde(default)]
    statics: Vec<DumpStatic>,
    #[serde(default)]
    water_planes: Vec<DumpWaterPlane>,
    land_textures: HashMap<String, DumpTexture>,
    cells: Vec<DumpCell>,
}

#[derive(Deserialize)]
struct DumpWorld {
    default_water_height: Option<f32>,
}

#[derive(Deserialize)]
struct DumpMarker {
    name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    /// Faction reputation tied to the place (New Vegas).
    reputation: Option<String>,
    x: f32,
    y: f32,
}

/// A marker of a child worldspace, with that worldspace's ONAM map scale and offset.
#[derive(Deserialize)]
struct DumpChildMarker {
    #[serde(flatten)]
    marker: DumpMarker,
    map_scale: Option<f32>,
    map_offset: Option<[f32; 2]>,
}

#[derive(Deserialize)]
struct DumpRadiation {
    radius: Option<f32>,
    x: f32,
    y: f32,
}

/// A placed static or tree: footprint in square units, and its mesh path.
#[derive(Deserialize)]
struct DumpStatic {
    x: f32,
    y: f32,
    area: f32,
    model: String,
}

/// A placed water plane: centre, height, z rotation (radians, clockwise) and half extents.
#[derive(Deserialize)]
struct DumpWaterPlane {
    x: f32,
    y: f32,
    z: f32,
    rotation: f32,
    half_x: f32,
    half_y: f32,
}

impl DumpWaterPlane {
    /// Whether world point (x, y) lies under the plane's footprint.
    fn covers(&self, x: f32, y: f32) -> bool {
        let (dx, dy) = (x - self.x, y - self.y);
        let (sin, cos) = self.rotation.sin_cos();
        (dx * cos - dy * sin).abs() <= self.half_x && (dx * sin + dy * cos).abs() <= self.half_y
    }
}

#[derive(Deserialize)]
struct DumpTexture {
    edid: Option<String>,
    texture: Option<String>,
}

#[derive(Deserialize)]
struct DumpCell {
    x: i32,
    y: i32,
    water_height: Option<f32>,
    /// 33x33 absolute heights, row-major from the south-west corner.
    heights: Option<Vec<i32>>,
    #[serde(default)]
    layers: Option<Vec<DumpLayer>>,
}

#[derive(Deserialize)]
struct DumpLayer {
    /// 0 bottom-left, 1 bottom-right, 2 top-left, 3 top-right.
    quadrant: u8,
    layer: u16,
    texture: Option<String>,
    /// (vertex index in the 17x17 quadrant, opacity); None for the base layer.
    alpha: Option<Vec<(u16, f32)>>,
}

// --- Classification ---

/// Terrain a landscape texture shows, from its editor ID and diffuse path. Bare, rocky and gravel
/// ground is desert (Sand); paved and rubble ground is urban (City); lake and river beds are Sand too,
/// since water comes from the water level instead.
fn texture_terrain(edid: &str, path: &str) -> &'static str {
    let (e, p) = (edid.to_lowercase(), path.to_lowercase());
    let any = |words: &[&str]| words.iter().any(|w| e.contains(w) || p.contains(w));
    if any(&["snow"]) { return "Snow"; }
    if p.contains("grass") || p.contains("oasis") || p.contains("astroturf") { return "Grass"; }
    if any(&["asphalt", "pavement", "rubble", "litter", "plaster", "cobblestone", "citadel"]) {
        if !any(&["rockcanyonrubble", "rubblerock"]) { return "City"; } // rock debris is desert
    }
    "Sand"
}

/// Places the map shows by their gates, as one marker at the gates' mean position: Freeside (two gates
/// on the Mojave map) and New Vegas, the Strip (its gate sits in the Freeside worldspace).
const NV_PLACES: &[(&str, &[&str])] = &[
    ("Freeside", &["Freeside's East Gate", "Freeside's North Gate"]),
    ("New Vegas", &["The Strip North Gate"]),
];
/// New Vegas towns that have no faction reputation (every other town has one).
const NV_TOWNS: &[&str] = &["Primm", "Nipton", "Jacobstown", "Camp Searchlight", "Nelson", "New Vegas"];
/// Settlement-type markers tied to a reputation that are camps, farms, businesses or facilities, not towns.
const NV_NOT_TOWNS: &[&str] = &[
    "Powder Ganger Camp North", "Powder Ganger Camp East", "Powder Ganger Camp South", "Powder Ganger Camp West",
    "Crimson Caravan Company", "188 Trading Post", "NCR Sharecropper Farms", "HELIOS One",
    "NCR Correctional Facility",
];
/// Notable New Vegas landmarks.
const NV_LANDMARKS: &[&str] = &[
    "Hoover Dam", "Nellis Air Force Base", "Camp McCarran", "Hidden Valley", "The Fort", "Black Mountain",
    "REPCONN Test Site", "Mojave Outpost", "Quarry Junction",
];

/// Category of a map marker. Vaults come from the marker type; towns are the settlement-type places
/// tied to a faction reputation (plus the few towns without one, minus camps); then the notable
/// landmarks. Towns, vaults and landmarks appear on the main maps; everything else is `Other`.
fn marker_icon(name: &str, kind: &str, has_reputation: bool) -> IconType {
    let settlement_type = matches!(kind, "City" | "Settlement" | "Encampment" | "TownRuins");
    if kind == "Vault" { IconType::Vault }
    else if NV_LANDMARKS.contains(&name) { IconType::Landmark }
    else if NV_TOWNS.contains(&name) || (has_reputation && settlement_type && !NV_NOT_TOWNS.contains(&name)) { IconType::City }
    else { IconType::Other }
}

/// Share of the quadrant (17x17 vertices) each texture covers: layers composite bottom-up, each alpha
/// layer covering what lies below it by its opacity. `None` is the default land texture (no base layer).
fn quadrant_cover<'a>(layers: &[&'a DumpLayer]) -> HashMap<Option<&'a str>, f32> {
    let base = layers.iter().find(|l| l.alpha.is_none()).and_then(|l| l.texture.as_deref());
    let mut vertices: Vec<Vec<(Option<&str>, f32)>> = vec![vec![(base, 1.0)]; 17 * 17];
    let mut alphas: Vec<&&DumpLayer> = layers.iter().filter(|l| l.alpha.is_some()).collect();
    alphas.sort_by_key(|l| l.layer);
    for l in alphas {
        for &(pos, opacity) in l.alpha.as_ref().unwrap() {
            let Some(v) = vertices.get_mut(pos as usize) else { continue };
            let a = opacity.clamp(0.0, 1.0);
            v.iter_mut().for_each(|e| e.1 *= 1.0 - a);
            v.push((l.texture.as_deref(), a));
        }
    }
    let mut cover = HashMap::new();
    for (t, w) in vertices.into_iter().flatten() { *cover.entry(t).or_insert(0.0) += w / (17.0 * 17.0); }
    cover
}

/// Heights of one quadrant's 17x17 vertices, row-major from its south-west corner.
fn quadrant_heights(h: &[i32], q: u8) -> impl Fn(usize, usize) -> f32 + '_ {
    let (col0, row0) = ((q as usize & 1) * 16, (q as usize >> 1) * 16);
    move |r, c| h[(row0 + r) * 33 + col0 + c] as f32
}

/// Wet vertices of a cell (33x33, row-major from the south-west corner): below the cell's water level,
/// or under a placed water plane and below its surface.
fn cell_wet(cell: &DumpCell, h: &[i32], water: f32, planes: &[DumpWaterPlane]) -> Vec<bool> {
    let (cx, cy) = (cell.x as f32 * CELL, cell.y as f32 * CELL);
    let near: Vec<&DumpWaterPlane> = planes.iter().filter(|p| {
        let reach = p.half_x.hypot(p.half_y);
        p.x + reach >= cx && p.x - reach <= cx + CELL && p.y + reach >= cy && p.y - reach <= cy + CELL
    }).collect();
    (0..33 * 33).map(|i| {
        let (z, x, y) = (h[i] as f32, cx + (i % 33) as f32 * VERTEX_SPACING, cy + (i / 33) as f32 * VERTEX_SPACING);
        z < water || near.iter().any(|p| z < p.z && p.covers(x, y))
    }).collect()
}

/// Whether most of the quadrant's vertices are wet.
fn quadrant_wet(wet: &[bool], q: u8) -> bool {
    let (col0, row0) = ((q as usize & 1) * 16, (q as usize >> 1) * 16);
    (0..17).flat_map(|r| (0..17).map(move |c| (r, c))).filter(|&(r, c)| wet[(row0 + r) * 33 + col0 + c]).count() * 2 > 17 * 17
}

/// Mean gradient magnitude (rise over run) over the quadrant's 16x16 squares.
fn quadrant_slope(h: &[i32], q: u8) -> f32 {
    let at = quadrant_heights(h, q);
    let sum: f32 = (0..16).flat_map(|r| (0..16).map(move |c| (r, c)))
        .map(|(r, c)| (at(r, c + 1) - at(r, c)).hypot(at(r + 1, c) - at(r, c))).sum();
    sum / (16.0 * 16.0) / VERTEX_SPACING
}

/// Terrain class with the largest share of the quadrant's ground cover (the default land texture is desert).
fn cover_terrain(cover: &HashMap<Option<&str>, f32>, class: &HashMap<&str, &'static str>) -> &'static str {
    let mut shares: HashMap<&'static str, f32> = HashMap::new();
    for (t, w) in cover {
        let terrain = t.and_then(|t| class.get(t).copied()).unwrap_or("Sand");
        *shares.entry(terrain).or_default() += w;
    }
    shares.into_iter().max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(a.0))).map_or("Sand", |(t, _)| t)
}

fn build(dump: &Dump) -> RawMapData {
    let land: Vec<&DumpCell> = dump.cells.iter().filter(|c| c.heights.is_some() || c.layers.is_some()).collect();
    if land.is_empty() {
        return RawMapData { warnings: vec!["worldspace has no landscape".into()], coordinate_scale: QUAD, priority: 1, ..Default::default() };
    }
    let x0 = land.iter().map(|c| c.x).min().unwrap();
    let x1 = land.iter().map(|c| c.x).max().unwrap();
    let y0 = land.iter().map(|c| c.y).min().unwrap();
    let y1 = land.iter().map(|c| c.y).max().unwrap();
    // World units -> frame units (origin at the north-west corner, y south).
    let to_frame = |x: f32, y: f32| (x - x0 as f32 * CELL, (y1 + 1) as f32 * CELL - y);

    let class: HashMap<&str, &'static str> = dump.land_textures.iter()
        .map(|(k, t)| (k.as_str(), texture_terrain(t.edid.as_deref().unwrap_or(""), t.texture.as_deref().unwrap_or(""))))
        .collect();
    let default_water = dump.worldspace.default_water_height.unwrap_or(f32::MIN);

    // Per quadrant tile: (wet, slope, ground cover terrain).
    let mut quads: HashMap<(i32, i32), (bool, Option<f32>, &'static str)> = HashMap::new();
    for cell in &land {
        let water = cell.water_height.filter(|&w| w < NO_WATER).unwrap_or(default_water);
        let layers = cell.layers.as_deref().unwrap_or(&[]);
        let h = cell.heights.as_deref();
        let wet = h.map(|h| cell_wet(cell, h, water, &dump.water_planes));
        for q in 0..4u8 {
            let in_quad: Vec<&DumpLayer> = layers.iter().filter(|l| l.quadrant == q).collect();
            let pos = ((cell.x - x0) * 2 + (q & 1) as i32, (y1 - cell.y) * 2 + 1 - (q >> 1) as i32);
            quads.insert(pos, (wet.as_deref().is_some_and(|w| quadrant_wet(w, q)), h.map(|h| quadrant_slope(h, q)), cover_terrain(&quadrant_cover(&in_quad), &class)));
        }
    }
    // Footprint of buildings and lush plants per quadrant, as a share of its ground area.
    let mut footprint: HashMap<(&'static str, (i32, i32)), f32> = HashMap::new();
    for o in &dump.statics {
        let Some(kind) = static_terrain(&o.model) else { continue };
        let (fx, fy) = to_frame(o.x, o.y);
        *footprint.entry((kind, ((fx / QUAD).floor() as i32, (fy / QUAD).floor() as i32))).or_default() += o.area / (QUAD * QUAD);
    }
    // Water first; then overgrowth, mountain and city where the footprint or slope averaged over the
    // 3x3 quadrants around is high (so ranges, built-up and overgrown areas count, not single bluffs,
    // shacks or plants); else the ground cover.
    let mut tiles: Vec<RawTile> = quads.iter().map(|(&(x, y), &(wet, _, ground))| {
        let around = || (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy)));
        let slopes: Vec<f32> = around().filter_map(|p| quads.get(&p).and_then(|q| q.1)).collect();
        let steep = !slopes.is_empty() && slopes.iter().sum::<f32>() / slopes.len() as f32 > MOUNTAIN_SLOPE;
        let share = |kind| around().map(|p| footprint.get(&(kind, p)).copied().unwrap_or(0.0)).sum::<f32>() / 9.0;
        // Overgrowth shows even on a hillside, such as Vault 22's.
        let terrain = if wet { "Water" } else if share("Grass") >= VEGETATION_FOOTPRINT { "Grass" } else if steep { "Mountain" }
            else if share("City") >= CITY_FOOTPRINT { "City" } else { ground };
        RawTile { x, y, tile_type: terrain.to_string(), real_lat: None, real_lon: None, weight: 1.0, anchor: None }
    }).collect();
    tiles.sort_by_key(|t| (t.y, t.x)); // deterministic order

    // Radiation: quadrants whose centre lies inside a radiation marker's radius (area-weighted, so
    // small hazards such as a leaking barrel cover no quadrant and cast no vote).
    let mut by_pos: HashMap<(i32, i32), usize> = tiles.iter().enumerate().map(|(i, t)| ((t.x, t.y), i)).collect();
    let mut irradiated = 0;
    for r in &dump.radiation {
        let Some(radius) = r.radius.filter(|&v| v > 0.0) else { continue };
        let (fx, fy) = to_frame(r.x, r.y);
        let span = (radius / QUAD).ceil() as i32 + 1;
        let (cx, cy) = ((fx / QUAD).floor() as i32, (fy / QUAD).floor() as i32);
        for ty in cy - span..=cy + span {
            for tx in cx - span..=cx + span {
                let centre = ((tx as f32 + 0.5) * QUAD, (ty as f32 + 0.5) * QUAD);
                if (centre.0 - fx).hypot(centre.1 - fy) > radius { continue; }
                match by_pos.get(&(tx, ty)) {
                    Some(&i) if tiles[i].tile_type == "Water" => {}
                    Some(&i) => { tiles[i].tile_type = "Radiation".into(); irradiated += 1; }
                    None => {
                        by_pos.insert((tx, ty), tiles.len());
                        tiles.push(RawTile { x: tx, y: ty, tile_type: "Radiation".into(), real_lat: None, real_lon: None, weight: 1.0, anchor: None });
                        irradiated += 1;
                    }
                }
            }
        }
    }

    // Markers of child worldspaces (Freeside...) sit on this map at position * scale + offset.
    let child = dump.child_markers.iter().map(|m| {
        let (scale, [ox, oy]) = (m.map_scale.unwrap_or(1.0), m.map_offset.unwrap_or([0.0, 0.0]));
        (&m.marker, m.marker.x * scale + ox, m.marker.y * scale + oy)
    });
    let mut locations: Vec<RawLocation> = Vec::new();
    let mut members: HashMap<String, usize> = HashMap::new(); // gates merged so far, per place
    for (m, wx, wy) in dump.markers.iter().map(|m| (m, m.x, m.y)).chain(child) {
        let Some(name) = m.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) else { continue };
        let name = NV_PLACES.iter().find(|(_, gates)| gates.contains(&name)).map_or(name, |(place, _)| place);
        let (x, y) = to_frame(wx, wy);
        let icon_type = marker_icon(name, m.kind.as_deref().unwrap_or("None"), m.reputation.is_some());
        if let Some(l) = locations.iter_mut().find(|l| l.name == name) {
            // Another gate of the same place: running mean of the gate positions.
            let n = members.entry(name.to_string()).or_insert(1);
            *n += 1;
            l.x += (x - l.x) / *n as f32;
            l.y += (y - l.y) / *n as f32;
            continue;
        }
        locations.push(RawLocation { name: name.to_string(), x, y, icon_type, real_lat: None, real_lon: None, terrain_override: None, anchor: None });
    }

    let (width, height) = (((x1 - x0 + 1) * 2) as u32, ((y1 - y0 + 1) * 2) as u32);
    let mut summary: HashMap<&str, usize> = HashMap::new();
    for t in &tiles { *summary.entry(t.tile_type.as_str()).or_default() += 1; }
    info!("[Bethesda] {} markers, {} cells, {}x{} tiles ({} irradiated): {:?}", locations.len(), land.len(), width, height, irradiated, summary);

    RawMapData {
        locations,
        tiles,
        width,
        height,
        world_bounds: Some(MapBounds { min_x: 0, min_y: 0, max_x: width as i32 - 1, max_y: height as i32 - 1 }),
        coordinate_scale: QUAD,
        priority: 1,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_classes() {
        assert_eq!(texture_terrain("LAsphaltWasteland01", "Textures/Landscape/AsphaltWasteland01.dds"), "City");
        assert_eq!(texture_terrain("LRubblePile03", "Textures/Landscape/RubblePile03.dds"), "City");
        assert_eq!(texture_terrain("GravelRoadWasteland01", "Textures/Landscape/Roads/GravelRoadWasteland01.dds"), "Sand");
        assert_eq!(texture_terrain("LWaterSandNV01", "Textures/landscape/WaterSandNV01.dds"), "Sand");
        assert_eq!(texture_terrain("LRockWastelandNV", "Textures/landscape/NV_Rocks/NV_RockFormations_02_D.dds"), "Sand");
        assert_eq!(texture_terrain("LRockCanyonRubble01", "Textures/Landscape/RockCanyonRubble01.dds"), "Sand");
        assert_eq!(texture_terrain("LGrassGolfNV", "Textures/landscape/grassgolfcourseNV01.dds"), "Grass");
        assert_eq!(texture_terrain("ScrubBarrenWastes01", "Textures/Landscape/ScrubWastes01.dds"), "Sand");
        assert_eq!(texture_terrain("LGrassGolfTallNV", "Textures/Landscape/ScrubDirtCanyon01.dds"), "Sand");
        assert_eq!(texture_terrain("LSnowSoftSnow", "Textures/Landscape/DLCAnch/DLCAnch_snow_softsnow.dds"), "Snow");
    }

    #[test]
    fn static_classes() {
        assert_eq!(static_terrain("Architecture\\Strip\\Imposter\\Bldg03_Imposter02.NIF"), Some("City"));
        assert_eq!(static_terrain("Architecture\\Urban\\RubblePiles\\Pile01.NIF"), Some("City"));
        assert_eq!(static_terrain("Architecture\\Wasteland\\Highway\\Overpass01.NIF"), None);
        assert_eq!(static_terrain("Landscape\\Plants\\Vault22Grass.NIF"), Some("Grass"));
        assert_eq!(static_terrain("Landscape\\Trees\\NV_WestJuniper02.NIF"), None);
        assert_eq!(static_terrain("Landscape\\Plants\\NVFicus01.NIF"), Some("Grass"));
        assert_eq!(static_terrain("Landscape\\Plants\\YuccaANV.NIF"), None);
        assert_eq!(static_terrain("Landscape\\Rocks\\NV_Rock01.NIF"), None);
    }

    #[test]
    fn marker_categories() {
        assert!(matches!(marker_icon("Vault 11", "Vault", false), IconType::Vault));
        assert!(matches!(marker_icon("Goodsprings", "Encampment", true), IconType::City));
        assert!(matches!(marker_icon("Primm", "City", false), IconType::City));
        assert!(matches!(marker_icon("New Vegas", "City", false), IconType::City));
        assert!(matches!(marker_icon("HELIOS One", "City", true), IconType::Other));
        assert!(matches!(marker_icon("Camp Forlorn Hope", "Military", true), IconType::Other));
        assert!(matches!(marker_icon("Hoover Dam", "Monument", true), IconType::Landmark));
        assert!(matches!(marker_icon("Goodsprings Cemetery", "NaturalLandmark", false), IconType::Other));
        assert!(matches!(marker_icon("Ranger Station Alpha", "Military", true), IconType::Other));
        assert!(matches!(marker_icon("Powder Ganger Camp East", "Encampment", true), IconType::Other));
    }

    fn layer(q: u8, n: u16, tex: &str, alpha: Option<Vec<(u16, f32)>>) -> DumpLayer {
        DumpLayer { quadrant: q, layer: n, texture: Some(tex.into()), alpha }
    }

    #[test]
    fn cover_composites_layers_by_opacity() {
        // Rock fully over half the vertices, then road at 50% over all of them.
        let rock: Vec<(u16, f32)> = (0..289).filter(|i| i % 2 == 0).map(|i| (i, 1.0)).collect();
        let road: Vec<(u16, f32)> = (0..289).map(|i| (i, 0.5)).collect();
        let layers = [layer(0, 0, "sand", None), layer(0, 1, "rock", Some(rock)), layer(0, 2, "road", Some(road))];
        let refs: Vec<&DumpLayer> = layers.iter().collect();
        let cover = quadrant_cover(&refs);
        let share = |t| cover.get(&Some(t)).copied().unwrap_or(0.0);
        assert!((share("road") - 0.5).abs() < 1e-4);
        assert!((share("rock") - 145.0 / 289.0 * 0.5).abs() < 1e-4);
        assert!((share("sand") + share("rock") + share("road") - 1.0).abs() < 1e-4);
        // No base layer: the default land texture.
        let none: Vec<&DumpLayer> = vec![];
        assert!((quadrant_cover(&none).get(&None).copied().unwrap() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn quadrant_rules() {
        let class: HashMap<&str, &'static str> = [("sand", "Sand"), ("city", "City")].into();
        let cover = |c: f32| -> HashMap<Option<&str>, f32> { [(Some("city"), c), (Some("sand"), 1.0 - c)].into() };
        assert_eq!(cover_terrain(&cover(0.6), &class), "City");
        assert_eq!(cover_terrain(&cover(0.3), &class), "Sand");
        assert_eq!(cover_terrain(&[(None, 1.0)].into(), &class), "Sand");
        // Flat cell at 100: dry under water level 50, wet under 200.
        let flat = vec![100; 33 * 33];
        let cell = DumpCell { x: 0, y: 0, water_height: None, heights: None, layers: None };
        assert!(!quadrant_wet(&cell_wet(&cell, &flat, 50.0, &[]), 3));
        assert!(quadrant_wet(&cell_wet(&cell, &flat, 200.0, &[]), 3));
        // A 45-degree-rotated square pond at 150 over the cell's south-west quadrant centre.
        let pond = DumpWaterPlane { x: 1024.0, y: 1024.0, z: 150.0, rotation: std::f32::consts::FRAC_PI_4, half_x: 1000.0, half_y: 1000.0 };
        assert!(pond.covers(1024.0 + 1300.0, 1024.0)); // inside the diamond's corner (1414 out)
        assert!(!pond.covers(1024.0 + 900.0, 1024.0 + 900.0)); // past its edge
        let wet = cell_wet(&cell, &flat, 50.0, &[pond]);
        assert!(quadrant_wet(&wet, 0));
        assert!(!quadrant_wet(&wet, 3));
        // Rising 96 units per 128-unit vertex step: slope 0.75.
        let hilly: Vec<i32> = (0..33 * 33).map(|i| (i % 33) as i32 * 96).collect();
        assert!((quadrant_slope(&hilly, 0) - 0.75).abs() < 1e-4);
        assert_eq!(quadrant_slope(&flat, 2), 0.0);
    }

    #[test]
    fn quadrant_tiles_and_marker_frame() {
        // One cell at (0, 0) with base texture per quadrant; the frame's origin is its north-west corner.
        let tex = |q| layer(q, 0, "t", None);
        let marker = |name: &str, x, y| DumpMarker { name: Some(name.into()), kind: Some("Encampment".into()), reputation: None, x, y };
        let dump = Dump {
            worldspace: DumpWorld { default_water_height: None },
            markers: vec![marker("Goodsprings", 1000.0, 3000.0)],
            // A child worldspace marker at (100, 200), scale 0.5, offset (2000, 1000) -> world (2050, 1100).
            child_markers: vec![DumpChildMarker { marker: marker("Gate", 100.0, 200.0), map_scale: Some(0.5), map_offset: Some([2000.0, 1000.0]) }],
            radiation: vec![],
            statics: vec![],
            water_planes: vec![],
            land_textures: [("t".to_string(), DumpTexture { edid: Some("Dirt01".into()), texture: None })].into(),
            cells: vec![DumpCell { x: 0, y: 0, water_height: None, heights: None, layers: Some((0..4).map(tex).collect()) }],
        };
        let map = build(&dump);
        assert_eq!((map.width, map.height), (2, 2));
        let mut pos: Vec<(i32, i32)> = map.tiles.iter().map(|t| (t.x, t.y)).collect();
        pos.sort();
        assert_eq!(pos, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
        // (1000, 3000) is in the cell's top-left quadrant: frame (1000, 1096).
        assert_eq!((map.locations[0].x, map.locations[0].y), (1000.0, 1096.0));
        assert_eq!((map.locations[1].x, map.locations[1].y), (2050.0, 2996.0));
    }
}
