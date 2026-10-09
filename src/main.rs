mod tile_gen;
mod tiled_structs;
mod map_gen;
mod schema;
mod extractors;
mod fixer;
mod pipeline;
mod rubbersheeting;
mod dat_extractor;
mod geocoding;
mod mods;
mod caching;
mod clustering; // Added clustering module
mod utils;
mod frm;
mod raster;
mod zar;
mod borders;
mod render;

use std::path::Path;
use std::fs;
use crate::extractors::Extractor;
use crate::extractors::interplay::{FalloutInterplayExtractor, GameVariant};

use simplelog::*;
use log::info;
use std::fs::File;

fn main() -> std::io::Result<()> {
    // Initialize logging
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .build();

    // Create output directory first
    fs::create_dir_all("output")?;

    let log_file = File::create("output/fallout_map.log")?;

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Info, config.clone(), TerminalMode::Mixed, ColorChoice::Auto),
            WriteLogger::new(LevelFilter::Info, config, log_file),
        ]
    ).unwrap();

    let tiles_dir = Path::new("output/tiles");
    let maps_dir = Path::new("output/maps");

    if !tiles_dir.exists() {
        fs::create_dir_all(tiles_dir)?;
    }
    if !maps_dir.exists() {
        fs::create_dir_all(maps_dir)?;
    }

    info!("--- Phase 1: Asset Generation ---");
    tile_gen::generate_tiles(tiles_dir)?;

    info!("--- Phase 2: Pipeline Execution ---");
    let mut game_extractors: Vec<Box<dyn Extractor>> = vec![
        Box::new(FalloutInterplayExtractor::new(GameVariant::Fallout1)),
        Box::new(FalloutInterplayExtractor::new(GameVariant::Fallout2)),
        Box::new(crate::extractors::van_buren::VanBurenExtractor),
        Box::new(crate::extractors::tactics::TacticsExtractor::Main),
        // Fallout 2–engine total conversions (tier 6): terrain only, their markers anchor the warp.
        Box::new(FalloutInterplayExtractor::new(GameVariant::FalloutNevada)),
        Box::new(FalloutInterplayExtractor::new(GameVariant::FalloutSonora)),
        Box::new(FalloutInterplayExtractor::new(GameVariant::FalloutResurrection)),
        // Fallout Tactics mods (tier 6): Crusade (the Texas Gulf Coast at Victoria).
        Box::new(crate::extractors::tactics::TacticsExtractor::Mod("fot_crusade")),
    ];

    // Bethesda games via Mutagen (all official plugins loaded; one worldspace placed per extractor).
    use crate::extractors::bethesda::{BethesdaExtractor, BethesdaGameVariant};
    // The Capital Wasteland, with the child worlds built on its coordinates that have map markers: DC's
    // ruins, Megaton, Bailey's Crossroads (not its rooftop views, walled-off scenes or test worlds).
    game_extractors.push(Box::new(BethesdaExtractor::parts(BethesdaGameVariant::Fallout3, &[
        "Wasteland", "DCworld01", "DCworld03", "DCworld05", "DCworld06", "DCworld08", "DCworld09", "DCworld10",
        "DCworld11", "DCworld12", "DCworld15", "DCworld17", "DCworld18", "MegatonWorld", "DLC02BaileysCrossroads",
    ])));
    // Pittsburgh, with its districts (the Steelyard, Downtown, Uptown): child worlds on its coordinates.
    game_extractors.push(Box::new(BethesdaExtractor::dlc_group(BethesdaGameVariant::Fallout3, "The Pitt", &[
        "DLC01PittWorld", "DLC01SteelMillExterior", "DLC01MarketSquare", "DLC01Haven",
    ])));
    // Operation Anchorage: a simulation of the 2077 Anchorage front, so weaker than Fallout 76. The battle
    // with the Artillery Overlook and the Chinese HQ, built on its coordinates; not the Glacier (the
    // opening cliff climb, other land on coordinates of its own).
    game_extractors.push(Box::new(BethesdaExtractor::dlc_group(BethesdaGameVariant::Fallout3, "Operation Anchorage", &[
        "DLC02AnchorageBattle", "DLC02Overlook", "DLC02ChineseHQ",
    ]).with_tier(crate::schema::tier::PAST_SIMULATION)));
    game_extractors.push(Box::new(BethesdaExtractor::dlc_group(BethesdaGameVariant::Fallout3, "Point Lookout", &[ // Maryland
        "DLC4Pointlookout", "DLC4Bog",
    ])));
    // Fallout 3 mods (tier 6). Into the Ark: Newark, New Jersey (the Ironbound a child world on its
    // coordinates; not the Ark, a separate little world, nor Philadelphia's, which has no landscape).
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout3, "into_the_ark", &["Newark.esp"], "Newark", &[
        "DLC06NewarkWastes", "DLC06Ironbound",
    ])));
    // Alton IL: Alton, Illinois, placed by the town at Fallout 3's scale (its places are laid out
    // freely: its Godfrey lies south of the river, the real one north); its other worlds, each on its own
    // coordinates, by the real places they name (not Belleview nor the BoS Nature Preserve: fictional).
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout3, "alton_il", &["Alton, IL.esm"], "Alton", &["AltonDowntownWorld"]).placed_by_name()));
    for (name, world) in [("New Godfrey", "AltonBoneyardWorld"), ("Dulles International Airport", "AltonPlaneDiscovery")] {
        game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout3, "alton_il", &["Alton, IL.esm"], name, &[world]).another_world().placed_by_name()));
    }
    // Project Eastern Wasteland: Riverdale, Maryland, the town and the wasteland around it, separate worlds
    // (not its Eastern Ruins: a 64-cell square of default land, built in a few rows).
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout3, "eastern_wasteland", &["Eastern Ruins.esp"], "Riverdale", &["Riverdale"]).placed_by_name()));
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout3, "eastern_wasteland", &["Eastern Ruins.esp"], "Riverdale Wasteland", &["RiverdaleWasteland"]).another_world().placed_by_name()));
    // DC Anniversary: the cut DC and Virginia districts restored, child worlds on the Wasteland's coordinates.
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::Fallout3, "dc_anniversary", &["DCAnniversary.esm"], &[
        "Wasteland", "DCworld02", "DCworld04", "DCworld07", "DCworld13", "DCworld14", "DCworld16", "DCworld19", "DCworld20",
        "DCworld21", "DCworld22", "DCworld23", "DCworld24", "DCworld25", "DCworld27", "AlexCityHallCourtyardWorld",
    ])));
    // Land past the Wasteland's north-western border: Vault 89 at Burkittsville, Maryland, and a forest
    // beyond Raven Rock.
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::Fallout3, "vault_89", &["Fallout 3 Vault 89 Mod - Master.esp"], &["Wasteland"])));
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::Fallout3, "northwest_forest", &["Northwest Forest Redone.esp"], &["Wasteland"])));
    game_extractors.push(Box::new(BethesdaExtractor::new(BethesdaGameVariant::FalloutNV, "WastelandNV")));
    // New Vegas mods (tier 6). New California: the Pass, the San Bernardino Mountains from the Cajon Pass to
    // Big Bear Lake, anchored by its real places; only the land within 3 cells of them (its world stretches
    // ~135 mi, far past the real places it depicts). Its radiation only from markers of 10 rads or more: 21 of
    // its markers are ambient zones of 5 rads over ~3 mi each, which would paint ~350 sq mi.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "new_california", &["NewCalifornia.esm"], "New California", &["00BlackBearCanyon01"]).radiation_from(10.0).within_anchors(3)));
    // Fallout Atlanta: Atlanta, Georgia (its city centre a child world on its coordinates), anchored by its real places.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "atlanta", &["Atlanta.esm"], "Atlanta", &["Atlanta", "AtlantaCityCentral"])));
    // Land added to the Mojave, placed by its own warp: the Mojave National Preserve south of Nipton
    // (Ghoulville), the land beyond its borders, and Arizona east of the Colorado.
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::FalloutNV, "ghoulville", &["Ghoulville.esp"], &["WastelandNV"])));
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::FalloutNV, "beyond_the_borders", &["BTB - Beyond the Borders.esp"], &["WastelandNV"])));
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::FalloutNV, "arizona_expedition", &["Arizona.esp"], &["WastelandNV"])));
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::FalloutNV, "nova_arizona", &["NovaArizona.esm"], &["WastelandNV"])));
    // Zion Trail: the walk from the Mojave to Zion (Nevada's Desert Wildlife Refuge to Zion National Park).
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "zion_trail", &["ZionTrail.esm"], "Zion Trail", &["ZionTrail"])));
    // Havasu Blues: Lake Havasu City, Arizona.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "havasu_blues", &["havasublues.esm"], "Lake Havasu", &["RadWorld01"])));
    // San Fran: San Francisco's north-east waterfront and the islands in the bay; its Chinatown ("Shi-town") a
    // world on its own coordinates, placed by its name.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "san_fran", &["SanFran.esm"], "San Francisco", &["SanFranWasteland"])));
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "san_fran", &["SanFran.esm"], "Chinatown", &["SanFranChinatownWorld"]).another_world().placed_by_name()));
    // Fallout Chicago (demo): the Loop and the Near North Side, with Union Station's railyard on its coordinates.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "fallout_chicago", &["Fallout Chicago V050.esm"], "Chicago", &["ChicagoWorld", "ModUnionStationExt"])));
    // Long 15 – NCR Expansion: the Long 15 before Lonesome Road's nuke, Baker, California and the land east of
    // it (with SR-127 north, a child world on its coordinates).
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "long_15", &["Long15Reloaded.esm"], "Long 15", &["ALLong15", "ALLong15NorthRoadWorld"])));
    // NWCE – Necropolis: its Carson City, Nevada, placed by its name at the game's scale (~8x5 mi, as the real
    // city). Not its Necropolis: at the game's scale ~20x35 mi for a ruined town, its true size under a mile.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "nwce", &["NWCEMod.esm"], "Carson City", &["CarsonCity"]).placed_by_name()));
    // The Empire Wasteland: New York (the Big Apple, with Washington Heights, Times Square, Jersey City and the
    // Polis Badlands as child worlds on its coordinates), anchored by its real places. Not its Hoboken ("Mile
    // Square City"): ~170 sq mi at the game's scale for a city of one.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "empire_wasteland", &["TheBigAppleMergedNew.esp"], "New York", &[
        "BigApple", "BigAppleStreetLevel", "BigAppleTimesSquare", "BigAppleJerseyCity", "BigAppleBadlands",
    ])));
    // The Frontier: Portland, Oregon, anchored by its real places; only the land within 3 cells of them (its
    // world is 128 cells square).
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "the_frontier", &["The Frontier.esm"], "Portland", &["thefrontier"]).within_anchors(3)));
    // Russell: Caruthers Canyon, in the New York Mountains of the Mojave National Preserve.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "russell", &["SomeguySeries.esm", "Russell.esp"], "Caruthers Canyon", &["aaCaruthersCanyonWorld"]).placed_by_name()));
    // Beyond Boulder Dome: Boulder, Colorado (a Van Buren place), its city, dome and the wastes around it,
    // child worlds of Boulder Mountain Park; not the Inner Boulder battle copy nor Outer Nevada.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "beyond_boulder_dome", &["domecity.esm"], "Boulder", &[
        "abBoulderLands1", "abDomeCity", "AJVInnerCity", "abCWastes", "abBrainCenterEXT", "abIANovaIndExt", "abIAMojaveExit", "DF1", "DF3TheAbyss",
    ]).placed_by_name()));
    // Denver: Dog City (Van Buren's name for it), and BFM Denver (not its Elitch Gardens, a world of ~10 mi
    // around a park of a few blocks, over Denver itself, nor its stadium, "the Haunted Sands"); BFM San
    // Francisco's outskirts. Their worlds have no markers: each is placed by its name.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "dog_city", &["Dog_City3.esp"], "Dog City", &["DogCityWorld"]).placed_by_name()));
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "bfm_denver", &["SpeedyResources.esm", "Denver.esm"], "Denver", &["DenverWorld"]).placed_by_name()));
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::FalloutNV, "bfm_san_francisco", &["SpeedyResources.esm", "San Francisco.esp"], "San Francisco", &["SanFranMainWorld"]).placed_by_name()));
    game_extractors.push(Box::new(BethesdaExtractor::new(BethesdaGameVariant::Fallout4, "Commonwealth"))); // Boston
    // Far Harbor: the Island, Mount Desert Island in Maine (not the DLC03VRWorldspace simulation).
    game_extractors.push(Box::new(BethesdaExtractor::dlc_group(BethesdaGameVariant::Fallout4, "Far Harbor", &["DLC03FarHarbor"])));
    game_extractors.push(Box::new(BethesdaExtractor::dlc(BethesdaGameVariant::Fallout4, "NukaWorld"))); // Nuka-World
    // Fallout 4 mods (tier 6). Land added to the Commonwealth, placed by its own warp: Cape Ann (the
    // Massachusetts Project, only around the anchors: it also reshapes the Commonwealth's far edges) and the
    // land past its borders (The Borders of the World).
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::Fallout4, "massachusetts_project", &["MSTP_MassachusettsProject.esp"], &["Commonwealth"]).within_anchors(3)));
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::Fallout4, "borders_of_the_world", &["Mozenrath - The Borders Of The World.esp"], &["Commonwealth"])));
    game_extractors.push(Box::new(BethesdaExtractor::mod_extension(BethesdaGameVariant::Fallout4, "beyond_the_borders_fo4", &["BTB- Beyond the Borders FO4.esp"], &["Commonwealth"])));
    // Fallout Miami (early version; the 1.03 update, whose plugin is the whole mod's): Miami, placed by the city at
    // Fallout 4's scale (its places are laid out freely: its Liberty District east of Lincoln Road, the real
    // Liberty City west of Miami Beach).
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout4, "fallout_miami", &["FalloutMiami.esp"], "Miami", &["Miami"]).placed_by_name()));
    // New England's states, each a world of its own, anchored by its real towns.
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout4, "vermont", &["VermontWorld.esp"], "Vermont", &["VermontWorld1"])));
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout4, "new_hampshire", &["newhampshireworld.esp"], "New Hampshire", &["newhampshireworld"])));
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout4, "connecticut", &["falloutconnecticut.esp"], "Connecticut", &["connecticutworld"])));
    // Project Empire: Puget Sound, Washington (its world is called "Northwest Commonwealth"), anchored by its
    // towns. Its land is a heightmap of the sound, sparsely built: all of it, trimmed to around its towns (its
    // world is 129 cells square).
    game_extractors.push(Box::new(BethesdaExtractor::from_mod(BethesdaGameVariant::Fallout4, "project_empire", &["ProjectEmpire-Merge.esm", "ProjectEmpire.esp"], "Puget Sound", &["ProjectEmpire"]).real_terrain().within_anchors(3)));
    // Fallout 76: Appalachia (West Virginia, with Skyline Valley and Burning Springs).
    game_extractors.push(Box::new(BethesdaExtractor::new(BethesdaGameVariant::Fallout76, "APPALACHIA").with_tier(crate::schema::tier::FALLOUT_76)));
    // Fallout 76's expeditions: interior zones dressed as outdoors, each placed at its real counterpart by its
    // name (not the Daily Ops copies, nor the zones' buildings within: the Casino and the Night Club). Each
    // city shows one marker; its other zones add terrain only.
    for (name, cell, ground, shown) in [
        // The Pitt (Pittsburgh), marked by Fallout 3's; the Pitt's own worldspace is an empty placeholder.
        ("The Pitt - Industrial", "XPDPitt01Industrial", "City", false), ("The Foundry", "XPDPitt01Foundry", "City", false),
        ("The Pitt - Trench", "XPDPitt02Trench", "City", false), ("Sanctum", "XPDPitt02Sanctum", "City", false),
        ("Steel Tower", "XPDRttP01SteelTower", "City", false),
        // Atlantic City (New Jersey; the game's region location), marked on its flooded downtown (under
        // water but for its buildings); generic zone names carry the city's.
        ("Atlantic City", "XPDAC03FloodedCityCenter", "Water", true), ("The Boardwalk", "XPDAC02Boardwalk", "City", false),
        ("Showman's Pier", "XPDAC02Pier", "City", false), ("Aquarium of the Atlantic", "XPDAC02Aquarium", "City", false),
        ("Casino Quarter", "XPDAC01EntertainmentDistrict", "City", false), ("Atlantic City Hall", "XPDAC03CityHall", "City", false),
        ("Atlantic City Community Center", "XPDAC03CommunityCenter", "City", false),
    ] {
        game_extractors.push(Box::new(BethesdaExtractor::zone(name, cell, ground, shown).with_tier(crate::schema::tier::FALLOUT_76)));
    }
    game_extractors.push(Box::new(BethesdaExtractor::dlc(BethesdaGameVariant::FalloutNV, "NVDLC02ZionCanyon"))); // Honest Hearts: Zion NP, Utah
    game_extractors.push(Box::new(BethesdaExtractor::dlc(BethesdaGameVariant::FalloutNV, "NVDLC03BigMT"))); // Old World Blues
    game_extractors.push(Box::new(BethesdaExtractor::dlc_group(BethesdaGameVariant::FalloutNV, "Sierra Madre", &[ // Dead Money
        "NVDLC01Villa", "NVDLC01WestTownN", "NVDLC01WestTownS", "NVDLC01EastTownN", "NVDLC01EastTownS",
        "NVDLC01VillaDean", "NVDLC01VillaChristine",
    ])));
    game_extractors.push(Box::new(BethesdaExtractor::dlc_group(BethesdaGameVariant::FalloutNV, "The Divide", &[ // Lonesome Road
        "NVDLC04DivideWorld", "NVDLC04Road01World", "NVDLC04Road02World", "NVDLC04NukeSilo2", "NVDLC04DivideVistaWorld",
    ])));

    // Recursively find all JSON files in input/manual
    fn find_manual_files(dir: &Path, files: &mut Vec<String>) -> std::io::Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    find_manual_files(&path, files)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    files.push(path.to_string_lossy().to_string());
                }
            }
        }
        Ok(())
    }

    let mut manual_files = Vec::new();
    let _ = find_manual_files(Path::new("input/manual"), &mut manual_files);
    
    let mut manual_extractors: Vec<Box<dyn Extractor>> = Vec::new();
    for file in manual_files {
        manual_extractors.push(Box::new(crate::extractors::manual::ManualExtractor::new(file, 6.0)));
    }

    let mut target_cluster = None;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--cluster" && i + 1 < args.len() {
            target_cluster = Some(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }

    if let Some(ref c) = target_cluster {
        info!("Running in single-cluster mode for: {}", c);
    }

    pipeline::run_pipeline(game_extractors, manual_extractors, maps_dir, target_cluster);

    info!("Done! stored in output/");
    Ok(())
}
