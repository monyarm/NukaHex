//! Fallout 76, read with fo76utils (MIT; built by build.rs into `.cache/tools/fo76utils`), since Mutagen
//! does not read it: `esmdump` lists SeventySix.esm's records as TSV, `btddump` decodes the landscape from
//! `Data/Terrain/<world>.btd` (Fallout 76 keeps it out of the plugin). The result is the same `Dump` the
//! Mutagen helper gives for the other games, so it is classified the same way.

use super::{Dump, DumpCell, DumpLayer, DumpMarker, DumpRadiation, DumpStatic, DumpTexture, DumpVolume, DumpWaterPlane, DumpWorld};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

const PLUGIN: &str = "SeventySix.esm";
const LOCALIZATION: &str = "SeventySix - Localization.ba2";
/// The engine's map marker base object.
const MAP_MARKER: u32 = 0x10;
/// Hazard radii are in feet; a foot is 64/3 game units.
const UNITS_PER_FOOT: f32 = 64.0 / 3.0;
/// btddump's level of detail 2: 32 samples per cell, 128 units apart (the vertex spacing of a cell's
/// 33x33 heights).
const LOD: u32 = 2;
/// Units per cell side.
const CELL_UNITS: f32 = 4096.0;
/// Ground height of an interior zone: unknown, so below any water plane in it.
const ZONE_GROUND: i32 = -1_000_000;
/// Where the esmdump passes are kept (see `esmdump`).
const WORK: &str = ".cache/bethesda/fo76";
const SAMPLES: i32 = 32;

/// The base records references are classified by.
struct Bases {
    /// Statics, collections, trees and plants: (footprint area, mesh).
    footprint: HashMap<u32, (f32, String)>,
    /// Land textures: (editor ID, material).
    ltex: HashMap<u32, (String, Option<String>)>,
    /// Hazards: (radius in units, name).
    hazards: HashMap<u32, (f32, String)>,
    /// Water planes (activators with a water type): half extents.
    water: HashMap<u32, (f32, f32)>,
    /// Activators' editor IDs (a trigger volume is known by its base's).
    activators: HashMap<u32, String>,
}

fn bases(esm: &Path, data_dir: &Path) -> Result<Bases, String> {
    // Footprints and meshes of statics, land textures (editor ID and material, when it has one; esmdump
    // prints LTEX's BNAM by default), hazards.
    let tsv = esmdump(esm, data_dir, Path::new(WORK), "bases", &["-i", "STAT", "-i", "SCOL", "-i", "TREE", "-i", "MSTT", "-i", "LTEX", "-i", "HAZD", "-i", "ACTI", "-i", "WATR"], "\
STAT,SCOL,TREE,MSTT,ACTI\tOBND\tBounds\tssssss
ACTI\tWNAM\tWater\td
SCOL\tONAM\tPart\td
SCOL\tDATA\tPlacements\t<fffffff
HAZD\tDNAM\tData\txf
")?;
    let mut b = Bases { footprint: HashMap::new(), ltex: HashMap::new(), hazards: HashMap::new(), water: HashMap::new(), activators: HashMap::new() };
    let mut collections: Vec<(u32, Vec<(u32, Vec<f32>)>)> = Vec::new(); // SCOL -> parts and their scales
    // Activators: half extents and the form IDs they print (one of them their water type, if any).
    let (mut activators, mut water_types): (Vec<(u32, (f32, f32), Vec<u32>)>, std::collections::HashSet<u32>) = Default::default();
    for_rows(&tsv, |rec, id, edid, full, data| {
        match rec {
            "STAT" | "SCOL" | "TREE" | "MSTT" => {
                // (Default fields such as the mesh are printed among the defined ones, in record order.)
                let bounds: Option<Vec<f32>> = data.windows(6).find(|w| w.iter().all(|v| v.parse::<i32>().is_ok()))
                    .map(|w| w.iter().map(|v| v.parse::<f32>().unwrap()).collect());
                // (esmdump writes paths with '/'; the games, and the mesh rules, use '\\'.)
                let model = data.iter().find(|v| v.to_lowercase().ends_with(".nif")).map(|m| m.replace('/', "\\"));
                if let (Some(bd), Some(m)) = (bounds, model) {
                    b.footprint.insert(id, (((bd[3] - bd[0]) * (bd[4] - bd[1])).abs(), m));
                }
                if rec == "SCOL" {
                    // ONAM part, then its placements (7 floats each, the 7th the scale), repeated.
                    let mut parts: Vec<(u32, Vec<f32>)> = Vec::new();
                    for v in data.iter().skip_while(|v| !v.starts_with("0x")) {
                        if let Some(part) = hex(v) { parts.push((part, Vec::new())); }
                        else if let (Some(p), Ok(f)) = (parts.last_mut(), v.parse::<f32>()) { p.1.push(f); }
                    }
                    collections.push((id, parts));
                }
            }
            "ACTI" => {
                b.activators.insert(id, edid.to_string());
                if let Some(bd) = data.windows(6).find(|w| w.iter().all(|v| v.parse::<i32>().is_ok())) {
                    let bd: Vec<f32> = bd.iter().map(|v| v.parse().unwrap()).collect();
                    activators.push((id, ((bd[3] - bd[0]).abs() / 2.0, (bd[4] - bd[1]).abs() / 2.0), data.iter().filter_map(|v| hex(v)).collect()));
                }
            }
            "WATR" => { water_types.insert(id); }
            "LTEX" => { b.ltex.insert(id, (edid.to_string(), data.iter().find(|v| v.contains('/')).map(|v| v.replace('/', "\\")))); }
            // DNAM: a hex count, then the radius (the mesh path may come first).
            "HAZD" => if let Some(r) = data.iter().position(|v| v.starts_with("0x")).and_then(|i| data.get(i + 1)).and_then(|v| v.parse::<f32>().ok()) {
                b.hazards.insert(id, (r * UNITS_PER_FOOT, format!("{} {}", edid, full)));
            },
            _ => {}
        }
    })?;
    for (id, half, ids) in activators {
        if ids.iter().any(|i| water_types.contains(i)) { b.water.insert(id, half); }
    }
    // A static collection's own mesh is an anonymous precombined one: it takes the mesh of its part with
    // the largest footprint over all its placements.
    for (id, parts) in collections {
        let dominant = parts.iter().filter_map(|(p, f)| b.footprint.get(p).map(|(a, m)| {
            let scales = f.chunks(7).filter_map(|pl| pl.get(6)).map(|s| s * s).sum::<f32>();
            (a * scales, m.clone())
        })).max_by(|a, b| a.0.total_cmp(&b.0));
        if let (Some((_, mesh)), Some(entry)) = (dominant, b.footprint.get_mut(&id)) { entry.1 = mesh; }
    }
    Ok(b)
}

/// Field definitions for placed references and the cells holding them.
const REFS_DEFS: &str = "\
REFR,PHZD\tNAME\tBase\td
REFR,PHZD\tDATA\tPosition\tffffff
CELL\tXCLC\tGrid\tii
CELL\tXCLW\tWater\tf
WRLD\tDNAM\tDefaults\tff
WRLD\tMNAM\tMap\tiissss
";
/// Field definitions for references' optional fields, each in a pass of its own (they would shift the
/// columns): scale, and the half extents of a primitive (a trigger volume's box).
const SCALES_DEFS: &str = "REFR,PHZD\tXSCL\tScale\tf\n";
const PRIMITIVES_DEFS: &str = "REFR\tXPRM\tPrimitive\tfff\n";

/// Per reference, the leading floats of an optional-field pass (`SCALES_DEFS`, `PRIMITIVES_DEFS`).
fn optional(tsv: &Path) -> Result<HashMap<u32, Vec<f32>>, String> {
    let mut values = HashMap::new();
    for_rows(tsv, |_, id, _, _, data| {
        let v: Vec<f32> = data.iter().map_while(|v| v.parse().ok()).collect();
        if !v.is_empty() { values.insert(id, v); }
    })?;
    Ok(values)
}

/// The optional fields of the references a pass `filter` selects: (scales, primitives' half extents).
fn extras(esm: &Path, data_dir: &Path, name: &str, filter: &[&str]) -> Result<(HashMap<u32, f32>, HashMap<u32, (f32, f32)>), String> {
    let work = Path::new(WORK);
    let scales = optional(&esmdump(esm, data_dir, work, &format!("{}_scales", name), filter, SCALES_DEFS)?)?;
    let primitives = optional(&esmdump(esm, data_dir, work, &format!("{}_primitives", name), filter, PRIMITIVES_DEFS)?)?;
    Ok((scales.into_iter().map(|(id, v)| (id, v[0])).collect(),
        primitives.into_iter().filter(|(_, v)| v.len() >= 2).map(|(id, v)| (id, (v[0], v[1]))).collect()))
}

/// What placed references contribute: map markers, statics, radiation, water planes, trigger volumes.
#[derive(Default)]
struct Placed { markers: Vec<DumpMarker>, statics: Vec<DumpStatic>, radiation: Vec<DumpRadiation>, water_planes: Vec<DumpWaterPlane>, volumes: Vec<DumpVolume> }

impl Placed {
    /// One REFR or PHZD row.
    fn add(&mut self, rec: &str, id: u32, edid: &str, full: &str, data: &[&str], bases: &Bases, (scale, primitives): &(HashMap<u32, f32>, HashMap<u32, (f32, f32)>)) {
        // NAME, then DATA (position, then rotation in radians); default fields printed in hex may come
        // between (most hazards' do).
        let placement: Vec<f32> = data.iter().skip(1).skip_while(|v| v.starts_with("0x")).take(6).map_while(|v| v.parse().ok()).collect();
        let (Some(base), &[x, y, z, _, _, rotation]) = (data.first().and_then(|v| hex(v)), &placement[..]) else { return };
        let s = scale.get(&id).copied().unwrap_or(1.0);
        if rec == "PHZD" {
            if let Some((r, what)) = bases.hazards.get(&base) {
                self.radiation.push(DumpRadiation { effect: Some(what.clone()), radius: Some(r * s), x, y });
            }
        } else if base == MAP_MARKER {
            // (Fallout 76's marker types are its own numbers: the kind is left out.)
            self.markers.push(DumpMarker { name: (!full.is_empty()).then(|| full.to_string()), edid: (!edid.is_empty()).then(|| edid.to_string()),
                kind: None, reputation: None, x, y });
        } else if let Some(&(half_x, half_y)) = primitives.get(&id) {
            self.volumes.push(DumpVolume { base: bases.activators.get(&base).cloned(), x, y, rotation, half_x: half_x * s, half_y: half_y * s });
        } else if let Some(&(half_x, half_y)) = bases.water.get(&base) {
            self.water_planes.push(DumpWaterPlane { x, y, z, rotation, half_x: half_x * s, half_y: half_y * s });
        } else if let Some((area, model)) = bases.footprint.get(&base) {
            if *area > 0.0 { self.statics.push(DumpStatic { x, y, area: area * s * s, model: model.clone() }); }
        }
    }
}

fn land_textures(bases: Bases) -> HashMap<String, DumpTexture> {
    bases.ltex.into_iter().map(|(id, (edid, material))| (format!("{:08X}", id), DumpTexture { edid: Some(edid), texture: material })).collect()
}

/// One worldspace (`world`, its editor ID) of the Fallout 76 install whose data folder is `data_dir`.
pub(super) fn dump(data_dir: &Path, world: &str) -> Result<Dump, String> {
    let work = Path::new(WORK).join(world.to_lowercase());
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let esm = data_dir.join(PLUGIN);
    let bases = bases(&esm, data_dir)?;

    // The worldspaces' records, in file order: each WRLD record, then its cells and their references.
    let refs = esmdump(&esm, data_dir, Path::new(WORK), "world", &["-i", "WRLD", "-x", "NAVM", "-x", "ACHR", "-x", "LAND", "-x", "PGRE"], REFS_DEFS)?;
    let scale = extras(&esm, data_dir, "world", &["-i", "WRLD", "-x", "NAVM", "-x", "ACHR", "-x", "LAND", "-x", "PGRE", "-x", "CELL"])?;

    let mut in_world = false;
    let mut found = false;
    let mut defaults = (None, None);
    let mut map_area: Option<([i32; 2], [i32; 2])> = None;
    let mut name = None;
    let mut water: HashMap<(i32, i32), f32> = HashMap::new();
    let mut placed = Placed::default();
    for_rows(&refs, |rec, id, edid, full, data| {
        match rec {
            "WRLD" => {
                in_world = edid.eq_ignore_ascii_case(world);
                if in_world {
                    found = true;
                    name = (!full.is_empty()).then(|| full.to_string());
                    // DNAM (default land, water heights), then MNAM (usable size, the map's NW and SE cells);
                    // default fields printed in hex come first.
                    let nums: Vec<f32> = data.iter().filter(|v| !v.starts_with("0x")).filter_map(|v| v.parse().ok()).collect();
                    defaults = (nums.first().copied(), nums.get(1).copied());
                    if let Some(m) = nums.get(4..8) { map_area = Some(([m[0] as i32, m[1] as i32], [m[2] as i32, m[3] as i32])); }
                }
            }
            _ if !in_world => {}
            "CELL" => {
                let nums: Vec<&str> = data.iter().copied().filter(|v| !v.starts_with("0x")).collect();
                if let (Some(x), Some(y)) = (nums.first().and_then(|v| v.parse().ok()), nums.get(1).and_then(|v| v.parse().ok())) {
                    if let Some(w) = nums.get(2).and_then(|v| v.parse::<f32>().ok()) { water.insert((x, y), w); }
                }
            }
            "REFR" | "PHZD" => placed.add(rec, id, edid, full, data, &bases, &scale),
            _ => {}
        }
    })?;
    if !found { return Err(format!("worldspace {} not found in {}", world, PLUGIN)); }

    // (The terrain file runs well beyond the game's map, into LOD land: kept, as for the other games.)
    let cells = landscape(data_dir, world, &work, &water)?;
    Ok(Dump {
        worldspace: DumpWorld { name, edid: Some(world.to_string()), default_land_height: defaults.0, default_water_height: defaults.1, map_nw_cell: map_area.map(|a| a.0), map_se_cell: map_area.map(|a| a.1), default_ground: None },
        markers: placed.markers, child_markers: Vec::new(), radiation: placed.radiation, statics: placed.statics, water_planes: placed.water_planes, volumes: placed.volumes,
        land_textures: land_textures(bases), cells, lod: None,
    })
}

/// One interior zone dressed as outdoors (`cell`, its editor ID): the expeditions (the Pitt, Atlantic City)
/// are sets of these, each on coordinates of its own, with no landscape. Their ground is `ground` (they are
/// city districts: City, or Water where flooded) over the cells their references cover, and wet under
/// their water planes (its height unknown: below any of them).
pub(super) fn zone(data_dir: &Path, cell: &str, ground: &str) -> Result<Dump, String> {
    let esm = data_dir.join(PLUGIN);
    let bases = bases(&esm, data_dir)?;
    let work = Path::new(WORK);
    let refs = esmdump(&esm, data_dir, work, "interiors", &["-i", "CELL", "-x", "WRLD", "-x", "NAVM", "-x", "ACHR"], REFS_DEFS)?;
    let scale = extras(&esm, data_dir, "interior", &["-i", "CELL", "-x", "WRLD", "-x", "NAVM", "-x", "ACHR"])?;
    let (mut in_cell, mut found, mut name) = (false, false, None);
    let mut placed = Placed::default();
    for_rows(&refs, |rec, id, edid, full, data| match rec {
        "CELL" => {
            in_cell = edid.eq_ignore_ascii_case(cell);
            if in_cell { found = true; name = (!full.is_empty()).then(|| full.to_string()); }
        }
        "REFR" | "PHZD" if in_cell => placed.add(rec, id, edid, full, data, &bases, &scale),
        _ => {}
    })?;
    if !found { return Err(format!("interior cell {} not found in {}", cell, PLUGIN)); }
    let cell_of = |v: f32| (v / CELL_UNITS).floor() as i32;
    let covered: std::collections::BTreeSet<(i32, i32)> = placed.statics.iter().map(|o| (o.x, o.y))
        .chain(placed.water_planes.iter().map(|p| (p.x, p.y))).map(|(x, y)| (cell_of(x), cell_of(y))).collect();
    let cells = covered.into_iter().map(|(x, y)| DumpCell { x, y, water_height: None, heights: Some(vec![ZONE_GROUND; 33 * 33]), layers: Some(Vec::new()) }).collect();
    Ok(Dump {
        worldspace: DumpWorld { name, edid: Some(cell.to_string()), default_land_height: None, default_water_height: None, map_nw_cell: None, map_se_cell: None, default_ground: Some(ground.to_string()) },
        markers: placed.markers, child_markers: Vec::new(), radiation: placed.radiation, statics: placed.statics, water_planes: placed.water_planes, volumes: placed.volumes,
        land_textures: land_textures(bases), cells, lod: None,
    })
}

fn hex(v: &str) -> Option<u32> {
    v.split_whitespace().next()?.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok())
}

/// Runs esmdump with field definitions `defs` (TSV output, one row per record) into `<work>/<name>.tsv`;
/// a pass already made with the same definitions from the same plugin is reused.
fn esmdump(esm: &Path, data_dir: &Path, work: &Path, name: &str, filter: &[&str], defs: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let (defs_path, out) = (work.join(format!("{}.def", name)), work.join(format!("{}.tsv", name)));
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let fresh = std::fs::read_to_string(&defs_path).is_ok_and(|d| d == format!("{}{:?}", defs, filter))
        && modified(&out).zip(modified(esm)).is_some_and(|(o, e)| o >= e);
    if fresh { return Ok(out); }
    let fields = work.join(format!("{}.fields", name));
    std::fs::write(&fields, defs).map_err(|e| e.to_string())?;
    let status = Command::new(env!("FO76UTILS_ESMDUMP")).arg(esm).arg(data_dir.join(LOCALIZATION))
        .args(["-t", "-n", "-F"]).arg(&fields).args(filter).arg("-o").arg(&out)
        .status().map_err(|e| format!("esmdump: {}", e))?;
    if !status.success() { return Err(format!("esmdump ({}) failed", name)); }
    std::fs::write(&defs_path, format!("{}{:?}", defs, filter)).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Each data row of an esmdump TSV: (record type, form ID, EDID, FULL, data columns).
fn for_rows(path: &Path, mut row: impl FnMut(&str, u32, &str, &str, &[&str])) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    for line in BufReader::new(file).lines().skip(1) {
        let line = line.map_err(|e| e.to_string())?;
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 7 { continue; }
        let Some(id) = hex(cols[2]) else { continue };
        row(cols[1], id, cols[3], cols[4], &cols[7..]);
    }
    Ok(())
}

/// The world cells' 33x33 heights and per-quadrant ground textures, from the terrain file. Its cells are
/// centred on the world's cell corners (its cell 0,0 is centred on the origin), so a world vertex is the
/// terrain sample 16 in from its cell's edge, and a world quadrant lies within one terrain quadrant.
fn landscape(data_dir: &Path, world: &str, work: &Path, water: &HashMap<(i32, i32), f32>) -> Result<Vec<DumpCell>, String> {
    let terrain = data_dir.join("Terrain");
    let btd = std::fs::read_dir(&terrain).map_err(|e| format!("{}: {}", terrain.display(), e))?.flatten().map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("btd")) && p.file_stem().is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(world)))
        .ok_or_else(|| format!("no {}.btd in {}", world, terrain.display()))?;
    let head = std::fs::read(&btd).map_err(|e| e.to_string())?;
    let (f32_at, i32_at) = (|o: usize| f32::from_le_bytes(head[o..o + 4].try_into().unwrap()), |o: usize| i32::from_le_bytes(head[o..o + 4].try_into().unwrap()));
    let (h_min, h_max) = (f32_at(8), f32_at(12));
    let (x0, y0, x1, y1) = (i32_at(24), i32_at(28), i32_at(32), i32_at(36));
    drop(head);
    let range = [x0, y0, x1, y1].map(|v| v.to_string());
    let image = |fmt: u32, lod: Option<u32>| -> Result<Vec<u8>, String> {
        let out = work.join(format!("btd{}.dds", fmt));
        let mut cmd = Command::new(env!("FO76UTILS_BTDDUMP"));
        cmd.arg(&btd).arg(&out).arg(fmt.to_string()).args(&range);
        if let Some(l) = lod { cmd.arg(l.to_string()); }
        if !cmd.stdout(std::process::Stdio::null()).status().map_err(|e| format!("btddump: {}", e))?.success() {
            return Err(format!("btddump format {} failed", fmt));
        }
        let data = std::fs::read(&out).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&out);
        Ok(data[128..].to_vec()) // after the DDS header
    };
    let (heights, alphas, sets) = (image(0, Some(LOD))?, image(2, Some(LOD))?, image(9, None)?);
    let (w, h) = ((x1 - x0 + 1) * SAMPLES, (y1 - y0 + 1) * SAMPLES);
    // Rasters run north to south; `g` counts samples from the south-west corner.
    let u16_at = |raster: &[u8], gx: i32, gy: i32| {
        let (c, r) = (gx.clamp(0, w - 1), (h - 1 - gy).clamp(0, h - 1));
        let o = 2 * (r * w + c) as usize;
        u16::from_le_bytes([raster[o], raster[o + 1]])
    };
    let set_at = |cx: i32, cy: i32, qx: i32, qy: i32, i: i32| {
        let (row, col) = ((y1 - cy) * 2 + (1 - qy), (cx - x0) * 32 + qx * 16 + i);
        let o = 4 * (row * (x1 - x0 + 1) * 32 + col) as usize;
        u32::from_le_bytes(sets[o..o + 4].try_into().unwrap())
    };
    let mut cells = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let (gx0, gy0) = ((x - x0) * SAMPLES + SAMPLES / 2, (y - y0) * SAMPLES + SAMPLES / 2);
            let heights: Vec<i32> = (0..33).flat_map(|vy| (0..33).map(move |vx| (vx, vy)))
                .map(|(vx, vy)| (h_min + u16_at(&heights, gx0 + vx, gy0 + vy) as f32 * (h_max - h_min) / 65535.0).round() as i32)
                .collect();
            let mut layers = Vec::new();
            for q in 0..4u8 {
                let (wx, wy) = ((q & 1) as i32, (q >> 1) as i32);
                // The world quadrant's terrain quadrant: its west/south half is the east/north half of terrain
                // cell (x, y); its east/north half the west/south half of the next.
                let (cx, qx) = if wx == 0 { (x, 1) } else { (x + 1, 0) };
                let (cy, qy) = if wy == 0 { (y, 1) } else { (y + 1, 0) };
                let slots: Vec<u32> = (0..6).map(|i| set_at(cx, cy, qx, qy, i)).collect();
                let mut cover = [0f32; 6];
                for sy in 0..SAMPLES / 2 {
                    for sx in 0..SAMPLES / 2 {
                        let a = u16_at(&alphas, gx0 + wx * SAMPLES / 2 + sx, gy0 + wy * SAMPLES / 2 + sy);
                        let mut share = [1f32, 0.0, 0.0, 0.0, 0.0, 0.0];
                        for n in 0..5 {
                            let op = ((a >> (3 * n)) & 7) as f32 / 7.0;
                            for s in share.iter_mut().take(n + 1) { *s *= 1.0 - op; }
                            share[n + 1] = op;
                        }
                        for k in 0..6 { cover[k] += share[k]; }
                    }
                }
                let best = (0..6).filter(|&k| slots[k] != 0).max_by(|&a, &b| cover[a].total_cmp(&cover[b]));
                if let Some(k) = best {
                    layers.push(DumpLayer { quadrant: q, layer: 0, texture: Some(format!("{:08X}", slots[k])), alpha: None });
                }
            }
            cells.push(DumpCell { x, y, water_height: water.get(&(x, y)).copied(), heights: Some(heights), layers: Some(layers) });
        }
    }
    Ok(cells)
}
