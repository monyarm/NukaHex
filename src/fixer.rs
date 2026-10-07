use crate::schema::{RawMapData, FixedMapData, FixedTile, FixedLocation, priority_weight};
use crate::rubbersheeting::{RubberSheet, ControlPoint};
use crate::geocoding;
use std::collections::{HashMap, HashSet};
use log::{info, warn};

// Real places the games' markers depict, by source: each group lists the clusters it anchors (a
// cluster id, or its prefix before a `_`: "fallout3" covers "fallout3_thepitt"), so a name only
// anchors the sources it was written for (Alton's "The Divide" is not Lonesome Road's). Each value is
// either coordinates ("37.376,-118.482") or a place name/address, geocoded.
const REAL_COORDS: &[(&[&str], &[(&str, &str)])] = &[
    (&["fallout_interplay", "mod_fallout2_fallout_nevada", "mod_fallout2_fallout_sonora", "mod_fallout2_fallout_resurrection"], &[
    // Fallout 1
        ("Shady Sands", "37.3767525,-118.4820828"),
        ("Mariposa Military Base", "36.624861300000006,-121.78344730000002"), 
        ("Lost Hill Bunker", "35.6081873,-119.3829345"),  
        ("Junktown", "35.65363980000001,-117.4329572"),
        ("The Hub", "34.1391501,-117.2897652"),  
        ("Necropolis", "34.913266,-117.015381"),  
        ("The Boneyard", "34.2070709,-118.3117676"),
        ("The Cathedral", "33.8088202,-118.32000730000001"),
        ("The Glow", "33.1467502,-116.85058590000001"),
        ("Vault 13", "37.3679428,-118.9736945"),
        ("Vault 15", "37.3964654,-118.2568593"),
        // Fallout 2
        ("Navarro", "38.8623645,-123.485783"),
        ("San Francisco", "37.7924224,-122.42614750000001"), 
        ("Redding", "40.5847565,-122.3876953"),
        ("Klamath", "41.5281142,-124.03907770000002"),
        ("Vault City", "41.1793239,-119.29021499999999"),
        ("Gecko", "41.7063278,-118.93786470000002"),
        ("Modoc", "41.6727239,-120.899064"),
        ("New Reno", "39.5294665,-119.89379880000001"),
        ("Broken Hills", "39.5380233,-118.9489428"),
    // Their names are the same places for the Fallout 2-engine mods set in that world (Vault City, New Reno).
    ]),
    (&["fallout_van_buren"], &[
        // Van Buren (real places on its world map; the rest are fictional and placed by the warp)
        ("Dog City", "Denver, Colorado"),
        ("Boulder", "Boulder, Colorado"),
        ("Hoover Dam", "Hoover Dam"),
        ("Grand Canyon", "Grand Canyon Village, Arizona"), // South Rim
        ("Mesa Verde", "Spruce Tree House, Colorado"), // Chapin Mesa, Mesa Verde NP
    ]),
    (&["fallout_tactics"], &[
        // Fallout Tactics (keys are the game's spellings; values the real towns). Not anchored:
        // Newton and Canyon City, whose in-game positions are 170-210 mi from their namesakes
        // (Canyon City sits in western Kansas); the warp places them.
        ("Freeport", "Freeport, Illinois"),
        ("Rock Falls", "Rock Falls, Illinois"),
        ("Macomb", "Macomb, Illinois"),
        ("Preoria", "Peoria, Illinois"),
        ("Quincy", "Quincy, Illinois"),
        ("Springfield", "Springfield, Illinois"),
        ("St.Louis", "St. Louis, Missouri"),
        ("Jefferson", "Jefferson City, Missouri"),
        ("Kansas City", "Kansas City, Missouri"),
        ("Osceolla", "Osceola, Missouri"),
        ("Junction City", "Junction City, Kansas"),
        ("Great Bend", "Great Bend, Kansas"),
        ("Coldwater", "Coldwater, Kansas"),
        ("Scott City", "Scott City, Kansas"),
        ("Beuna Vista", "Buena Vista, Colorado"),
        ("Cheyenne Mountain", "Cheyenne Mountain Complex"),
    ]),
    (&["falloutnv", "mod_falloutnv_ghoulville", "mod_falloutnv_beyond_the_borders", "mod_falloutnv_arizona_expedition", "mod_falloutnv_nova_arizona"], &[
        // Fallout: New Vegas (keys are the game's marker names; values the real places they depict).
        // Not anchored: fictional sites (Novac, Freeside, HELIOS One...) and sites whose real counterpart is
        // ambiguous (Jacobstown, Hidden Valley) or too close to another anchor (Techatticup Mine/Nelson).
        ("New Vegas", "Las Vegas Strip"),
        ("Hoover Dam", "Hoover Dam"),
        ("Goodsprings", "Goodsprings, Nevada"),
        ("Primm", "Primm, Nevada"),
        ("Nipton", "Nipton, California"),
        ("Jean Sky Diving", "Jean, Nevada"),
        ("Sloan", "Sloan, Nevada"),
        ("Ivanpah Dry Lake", "Ivanpah Lake, California"),
        ("Camp Searchlight", "Searchlight, Nevada"),
        ("Searchlight Airport", "Searchlight Airport, Nevada"),
        ("Cottonwood Cove", "Cottonwood Cove, Nevada"),
        ("Nelson", "Nelson, Nevada"),
        ("El Dorado Dry Lake", "Eldorado Dry Lake, Nevada"),
        ("Boulder City", "Boulder City, Nevada"),
        ("Black Mountain", "Black Mountain, Clark County, Nevada"),
        ("Boulder Beach Campground", "Boulder Beach, Lake Mead"),
        ("The Fort", "Fortification Hill, Arizona"),
        ("Lake Las Vegas", "Lake Las Vegas, Nevada"),
        ("Callville Bay", "Callville Bay, Nevada"),
        ("Camp McCarran", "Harry Reid International Airport"),
        ("Nellis Air Force Base", "Nellis Air Force Base"),
        ("Red Rock Canyon", "Red Rock Canyon Visitor Center, Nevada"),
        ("Bonnie Springs", "Bonnie Springs, Nevada"),
        ("Spring Mt. Ranch State Park", "Spring Mountain Ranch State Park"),
        // Honest Hearts (Zion Canyon worldspace: Zion National Park, Utah). The canyon-floor places lie
        // almost in a line, so the off-axis Narrows (Sorrows Camp is in them) and Three Marys keep the
        // warp from squashing the game's map. Not anchored: Old Rockville Bridge, which the game puts
        // north-west of the park entrance while Rockville lies south-west of it.
        ("Zion Valley Welcome Booth", "Zion Canyon Visitor Center"),
        ("Pine Creek Tunnel", "Zion-Mount Carmel Tunnel"),
        ("Patriarchs' Campground", "Court of the Patriarchs"),
        ("Zion Fishing Lodge", "Zion Lodge"),
        ("Sorrows Camp", "Temple of Sinawava"),             // mouth of the Narrows
        ("Three Marys", "37.2083156,-113.0118854"),         // Wikipedia; the name geocodes elsewhere
        // Lonesome Road (the Divide; fictional). Canon: devastated land in the north-western Mojave, near
        // the California/Nevada border, along Route 127 (the NCR's northern supply route), with Big MT to
        // its south. 127 crosses the border at Death Valley Junction.
        ("The Divide", "Death Valley Junction, California"),
        // Old World Blues (Big MT; fictional, its location deliberately unstated). South of the Divide
        // (Hopeville, in its south-west corner) and beside Route 127, which the NCR "couldn't take north"
        // past the Divide and the Big Empty: the south end of the Resting Spring Range, at Shoshone.
        ("Big MT", "Shoshone, California"),
        // Dead Money (the Sierra Madre; fictional, location never stated). "A mystic place close to the
        // Grand Canyon" (the Vault wiki) and "part of the Mojave" (the add-on's intro): where the Mojave
        // Desert meets the Grand Canyon, at Grand Canyon West.
        ("Sierra Madre", "Grand Canyon West, Arizona"),
    ]),
    // Fallout 2-engine mods (tier 6; keys are their in-game area names, map.msg 1500+). Their markers
    // only anchor the warp. Names shared with Fallout 2 (Vault City, New Reno) are the same places.
    (&["mod_fallout2_fallout_sonora"], &[
        // Fallout: Sonora. Dayglow is San Diego in canon.
        ("Flagstaff", "Flagstaff, Arizona"),
        ("Phoenix", "Phoenix, Arizona"),
        ("Casa Grande", "Casa Grande, Arizona"),
        ("Quartzsite", "Quartzsite, Arizona"),
        ("Hermosillo", "Hermosillo, Sonora, Mexico"),
        ("Puerto", "Puerto Peñasco, Sonora, Mexico"),          // "Puerto Penasco" in its worldmap text
        ("Santa Ana", "Santa Ana, Sonora, Mexico"),
        ("San Felipe", "San Felipe, Baja California, Mexico"),
        ("Dayglow", "San Diego, California"),
    ]),
    (&["mod_fallout2_fallout_nevada"], &[
        // Fallout: Nevada. Baydie and Ailie are its translation's spellings of Beatty and Ely. Not
        // anchored: fictional sites (Wind of War, the Sigma base, Uranium City, Old Rabbi...).
        ("Las Vegas", "Las Vegas, Nevada"),
        ("Area 51", "Area 51, Nevada"),
        ("Baydie", "Beatty, Nevada"),
        ("Ailie", "Ely, Nevada"),
        ("Hawthorne Military Base", "Hawthorne Army Depot"),
        ("Bridgeport", "Bridgeport, California"),
        ("Lovelock", "Lovelock, Nevada"),
        ("Gerlach", "Gerlach, Nevada"),
        ("Black Rock", "Black Rock Point, Nevada"),
        ("Battle Mountain", "Battle Mountain, Nevada"),
        ("Wendover", "Wendover, Utah"),
        ("Salt Lake City", "Salt Lake City, Utah"),
        ("Provo District", "Provo, Utah"),
    ]),
    (&["mod_fallout2_fallout_resurrection"], &[
        // Fallout 1.5: Resurrection (New Mexico). Rat Hole is Kirtland ("Kirtland... I mean Rat Hole, that's
        // what it's called now"; the town north-west of Albuquerque, as the game draws it, not Albuquerque's
        // Kirtland Air Force Base). Not anchored: its fictional towns (New Hope, Sedit, Corath...).
        ("Albuquerque", "Albuquerque, New Mexico"),
        ("Rat Hole", "36.7342,-108.3593"),                   // Kirtland, New Mexico
    ]),
    (&["mod_fallout_tactics_fot_crusade"], &[
        // Fallout Tactics: Crusade (the Texas Gulf Coast). Not anchored: Garcitas Creek (a creek, no one point:
        // 12 mi off the fit), La Salle and Guada (not placeable), its fictional sites.
        ("Victoria", "28.8053,-97.0036"),                    // Victoria, Texas (the city; the query finds the county)
        ("Bloomington", "Bloomington, Texas"),
        ("Port Lavaca", "Port Lavaca, Texas"),
        ("Vanderbilt", "Vanderbilt, Texas"),
    ]),
    (&["fallout3", "mod_fallout3_dc_anniversary", "mod_fallout3_vault_89", "mod_fallout3_northwest_forest"], &[
        // Fallout 3 (keys are the game's marker names; values the real places they depict). DC's ruins
        // (child worldspaces) share the Wasteland's coordinates. Not anchored: fictional places (Megaton,
        // Rivet City, the vaults...), and the Jefferson Memorial, the Capitol and the White House, 1-2 mi from
        // the Mall's two anchors in reality but 5+ mi in the game: the warp overshoots (folds) between them.
        ("The Citadel", "The Pentagon"),
        ("Lincoln Memorial", "Lincoln Memorial"),
        ("The Washington Monument", "Washington Monument"),
        ("Georgetown North", "Georgetown, Washington, D.C."),
        ("Anacostia Crossing", "Anacostia, Washington, D.C."),
        ("Takoma Park", "Takoma Park, Maryland"),
        ("Friendship Heights", "Friendship Heights Village, Montgomery County, Maryland"),
        ("Bethesda Ruins", "Bethesda, Maryland"),
        ("Wheaton Armory", "Wheaton, Maryland"),
        ("Old Olney", "Olney, Maryland"),
        ("Germantown Police HQ", "Germantown, Maryland"),
        ("Dickerson Tabernacle Chapel", "Dickerson, Maryland"),
        ("Falls Church Metro", "Falls Church, Virginia"),
        ("Fairfax Ruins", "Fairfax, Virginia"),
        ("Alexandria Arms", "Alexandria, Virginia"),
        ("Raven Rock", "Raven Rock Mountain Complex"),
        // The Pitt (its districts are child worlds sharing its frame) and Point Lookout (with the Sacred
        // Bog): each placed by one real place at Fallout 3's scale.
        ("The Pitt", "Downtown Pittsburgh"),                  // the game's Pitt Downtown
        ("Point Lookout Lighthouse", "Point Lookout Lighthouse, Maryland"),
        // Operation Anchorage: the simulated liberation of Anchorage, Alaska (2077).
        ("Operation Anchorage", "Anchorage, Alaska"),
    // Also the mods extending the Wasteland (DC Anniversary, Vault 89, Northwest Forest), whose land follows
    // the game's own warp.
    ]),
    // Fallout 3 mods.
    (&["mod_fallout3_vault_89"], &[
    // Vault 89: the Burkittsville vault of Hamilton's Hideaway's cut logs, past the Wasteland's north-western border.
        ("Vault 89", "Burkittsville, Maryland"),
    ]),
    (&["mod_fallout3_alton_il"], &[
        ("Alton", "Alton, Illinois"),
        ("New Godfrey", "Godfrey, Illinois"),
        ("Dulles International Airport", "Washington Dulles International Airport"),
    ]),
    (&["mod_fallout3_eastern_wasteland"], &[
        ("Riverdale", "Riverdale Park, Maryland"),
        ("Riverdale Wasteland", "Riverdale Park, Maryland"),
    ]),
    (&["mod_fallout3_into_the_ark"], &[
    // Newark (not anchored: its fictional places, and stations and buildings within a few blocks of these).
        ("Military Park", "Military Park, Newark"),
        ("Newark Penn Station", "Newark Penn Station"),
        ("Broad St. Station", "Newark Broad Street Station"),
        ("Essex Community College", "Essex County College, Newark"),
        ("Krating Daeng Stadium", "40.7368,-74.1503"),           // Red Bull Arena, Harrison (Krating Daeng: Thai Red Bull)
        ("Ironbound Entrance", "Ironbound, Newark"),
    ]),
    // New Vegas mods.
    (&["mod_falloutnv_new_california"], &[
        // The Pass, laid out as the real one: I-15 north from the Cajon Pass to Hesperia, Barstow beyond, the
        // Mormon Rocks below the pass, SR-138 east from I-15 to its junction with SR-18 at Crestline, Big Bear
        // Lake east (its two markers, on the lake's shores, merged). Not anchored: the I-138/I-15 Beltway (a
        // second spot for the same interchange), Big Bear Wharf (on the game's west shore; the real village is
        // on the south-east), the SR-18 forks (which junctions is unclear), Rice Army Airfield (the real one
        // lies 150 mi east, in the Colorado Desert) and its fictional places (Athens, Pinehaven, Union City...).
        ("Big Bear Lake", "Big Bear Lake, California"),
        ("I-15 Cajon Pass Bridge", "Cajon Pass, California"),
        ("I-15 - I-138 Black Bear Junction", "Cajon Junction, California"),
        ("Mormon Rocks", "Mormon Rocks, California"),
        ("The Junction", "Crestline, California"),
        ("I-15 North Bridge to Hesperia", "Hesperia, California"),
        ("Barstow", "Barstow, California"),
    ]),
    (&["mod_falloutnv_atlanta"], &[
        // Atlanta's neighbourhoods, parks and landmarks. Not anchored: its fictional sites (Bridge Town, the
        // caves...), and Windy Hill and Oakcliff, which the game puts south and west while the real ones (Smyrna,
        // Doraville) lie north-west and north-east.
        ("Stone Mountain", "Stone Mountain, Georgia"),
        ("Westview Cemetery", "Westview Cemetery, Atlanta"),
        ("Grant Park", "Grant Park, Atlanta"),
        ("East Lake Golf Club", "East Lake Golf Club, Atlanta"),
        ("Vine City", "Vine City, Atlanta"),
        ("People's Town", "Peoplestown, Atlanta"),
        ("Forest Park", "Forest Park, Georgia"),
        ("Riverdale", "Riverdale, Georgia"),
        ("Washington", "Washington Park, Atlanta"),
        ("Evergreen Lakeside Resort", "33.7994,-84.1302"),           // Atlanta Evergreen Lakeside Resort, Stone Mountain Park
        ("Starlight Drive-In", "33.7027,-84.3470"),                   // Starlight Drive-In, 2000 Moreland Ave SE
        ("North Clayton High School", "33.6096,-84.4396"),
    ]),
    (&["mod_falloutnv_ghoulville"], &[
        // The Mojave National Preserve: Kelso and the Dead Mountains, east of it, as in reality.
        ("Kelso", "Kelso Depot, California"),
        ("Dead Mountains", "Dead Mountains, California"),
    ]),
    (&["mod_falloutnv_arizona_expedition"], &[
        // Down the Arizona bank of the Colorado: Willow Beach below Hoover Dam, Bullhead City far south.
        ("Camp Willow", "Willow Beach, Arizona"),
        ("Bullhead Ruins", "Bullhead City, Arizona"),
    ]),
    (&["mod_falloutnv_nova_arizona"], &[
        // The Arizona side of the Colorado, laid out as the real one: Willow Beach, Arizona Hot Spring and the
        // Liberty Bell Arch by the river, Temple Bar on Lake Mead to the north, White Hills, Dolan Springs, then
        // Chloride and Grasshopper Junction to the south-east, Windy Point in the Cerbats. Not anchored: its
        // fictional sites, and the mines and camps whose real counterparts are unclear.
        ("Camp Willow", "Willow Beach, Arizona"),
        ("Arizona Hot Spring", "Arizona Hot Springs"),
        ("Liberty Bell Arch", "Liberty Bell Arch, Arizona"),
        ("Temple Bar Marina", "36.0344,-114.3223"),                  // Temple Bar, on Lake Mead
        ("White Hills", "White Hills, Arizona"),
        ("Dolan Springs", "Dolan Springs, Arizona"),
        ("Chloride", "Chloride, Arizona"),
        ("Grasshopper Junction", "Grasshopper Junction, Arizona"),
        ("Mount Perkins", "Mount Perkins, Arizona"),
        ("Windy Point Campground", "35.4362,-114.1604"),              // Windy Point, Cerbat Mountains
    ]),
    (&["mod_falloutnv_zion_trail"], &[
        // US 95 at the Desert National Wildlife Refuge's gate (Corn Creek), Mormon Well and Hayford Peak in the
        // refuge, Zion's south entrance and the park. Not anchored: Sawmill Junction (east of the refuge in the
        // game; the real Sawmill Canyon is north of Mormon Well) and its fictional places.
        ("Route 95 Check Point", "Corn Creek, Nevada"),
        ("Mormon Wells Campground", "Mormon Well, Nevada"),
        ("Hayford", "Hayford Peak, Nevada"),
        ("Zion Park Gates", "Zion Canyon Visitor Center"),
        ("Zion National Park", "Zion Lodge, Utah"),
    ]),
    (&["mod_falloutnv_havasu_blues"], &[
        // Lake Havasu City: Pittsburg Point (the island) and the airport north-east of it. Not anchored: Catfish
        // Bay and Site Six (real, but not found to place), the town's gates and its fictional sites.
        ("Pittsburg Point", "Pittsburg Point, Lake Havasu City"),
        ("Havasu Airstrip", "34.5714,-114.3591"),                     // Lake Havasu City Airport
    ]),
    (&["mod_falloutnv_san_fran"], &[
        // San Francisco's north-east waterfront and the bay. Not anchored: Pier 34 (beside Pier 33 in the game,
        // south of the Bay Bridge in reality), the Bay Bridge's tower (no one spot) and its fictional sites.
        ("Ferry Building", "Ferry Building, San Francisco"),
        ("The Embarcadero", "The Embarcadero, San Francisco"),
        ("BART Embarcadero Station", "Embarcadero station, San Francisco"),
        ("Pier 24 warehouse", "Pier 24, San Francisco"),
        ("Pier 33 warehouse", "Pier 33, San Francisco"),
        ("Pier 39", "Pier 39, San Francisco"),
        ("Telegraph Hill", "Telegraph Hill, San Francisco"),
        ("Coit tower apts", "Coit Tower, San Francisco"),
        ("North Beach", "North Beach, San Francisco"),
        ("Leavenworth St Station", "37.8055,-122.4186"),              // Leavenworth St at Fisherman's Wharf
        ("maritime park visitor center", "37.8075,-122.4205"),        // San Francisco Maritime Visitor Center, 499 Jefferson St
        ("Yerba Buena Island", "Yerba Buena Island"),
        ("Treasure Island", "Treasure Island, San Francisco"),
        ("Chinatown", "Chinatown, San Francisco"),
    ]),
    (&["mod_falloutnv_fallout_chicago"], &[
        // The Loop and the Near North Side, built near street scale (~5x4 real mi). Not anchored: Millennium Park
        // (~1.9 mi east of Willis Tower at the others' scale, 0.7 in reality), the Theater District (south of
        // Willis Tower in the game, north in reality), Trinity Hospital and the police headquarters (north in the game, on the South
        // Side in reality), Grand station, the signposts ("To Union Station", "To O'hare") and its fictional sites.
        ("Willis Tower", "Willis Tower, Chicago"),
        ("ChinaTown", "Chinatown, Chicago"),
        ("Cabrini Green", "Cabrini-Green, Chicago"),
        ("The Second City", "The Second City, Chicago"),
        ("Stanton Park", "Stanton Park, Chicago"),
        ("Marina", "Marina City, Chicago"),
        ("Portillos Barnelli's", "Portillo's, 100 West Ontario Street, Chicago"),
    ]),
    (&["mod_falloutnv_long_15"], &[
        // Baker and the land east of it, as the real: the Hollow Hills north-east, Kelso south-east. Not anchored:
        // Cima's cinder mines (a volcanic field, no one point), Soda Springs (east of Baker in the game; the real one, Zzyzx, is south-west),
        // Baker's streets and buildings (within a mile of each other; they would shrink the town) and its
        // fictional sites.
        ("Baker", "Baker, California"),
        ("Hollow Hills, North", "Hollow Hills Wilderness, California"),
        ("Kelso Crater", "Kelso, California"),
    ]),
    (&["mod_falloutnv_nwce"], &[("Carson City", "Carson City, Nevada")]),
    (&["mod_falloutnv_empire_wasteland"], &[
        // As the real city: the George Washington Bridge in Washington Heights, Times Square south of them, Jersey
        // City south-west across the Hudson. Not anchored: the Market District overpass and the Polis Badlands
        // (fictional).
        ("George Washington Bridge", "George Washington Bridge"),
        ("168 Washington Heights", "168th Street station, New York"),
        ("Time Square", "Times Square, New York"),
        ("Jersey City", "Jersey City, New Jersey"),
    ]),
    (&["mod_falloutnv_the_frontier"], &[
        // Portland, as the real one: Skidmore Fountain at the Burnside Bridge's west end, the Pearl District,
        // Old Town and Slabtown north of it, Nob Hill, Arlington Heights and Goose Hollow west, Council Crest
        // south-west, the Convention Center and Irvington north-east, the Marquam Bridge south. Not anchored:
        // South Waterfront (east in the game, on the west bank in reality), Lloyd Center (south; really
        // north-east), the airport (south-west; PDX is north-east), Buckman (north of Burnside; really south),
        // the White Stag sign (at the Burnside Bridge, already anchored), Hosford-Abernathy (not found to
        // place) and its fictional places.
        ("Burnside Bridge", "Burnside Bridge, Portland"),
        ("Skidmore Fountain", "Skidmore Fountain, Portland"),
        ("Pearl District", "Pearl District, Portland"),
        ("Old Town", "Old Town Chinatown, Portland"),
        ("Ground Kontrol Arcade", "Ground Kontrol, Portland"),
        ("Slab Town", "Slabtown, Portland"),
        ("Nob Hill", "Nob Hill, Portland"),
        ("Arlington Heights", "Arlington Heights, Portland"),
        ("Goose Hollow", "Goose Hollow, Portland"),
        ("Council Crest Park Facility", "Council Crest Park, Portland"),
        ("Portland Library", "Multnomah County Central Library"),
        ("Oregon Convention Centre", "Oregon Convention Center"),
        ("Irvington", "Irvington, Portland"),
        ("Marquam Bridge Interstate 5", "Marquam Bridge"),
    ]),
    (&["mod_falloutnv_russell"], &[("Caruthers Canyon", "Caruthers Canyon, California")]),
    (&["mod_falloutnv_beyond_boulder_dome"], &[("Boulder", "Boulder, Colorado")]),
    (&["mod_falloutnv_dog_city"], &[("Dog City", "Denver, Colorado")]),
    (&["mod_falloutnv_bfm_denver"], &[("Denver", "Denver, Colorado")]),
    (&["mod_falloutnv_bfm_san_francisco"], &[("San Francisco", "San Francisco, California")]),
    (&["fallout4"], &[
        // Fallout 4 (keys are the game's marker names; values the real places they depict). Not anchored:
        // fictional places; downtown landmarks within a mile of these; Rocky Narrows Park (the game's is
        // north-west of Boston, the real one in Sherborn to the south-west, ~13 mi off); Fort Strong (the
        // game's is 5 mi north of Spectacle Island, the real one 1 mi south-east of it); and buildings standing
        // for a town (Natick Banks, Milton General Hospital), which fold the warp against the real places beside.
        ("Diamond City", "Fenway Park"),
        ("Goodneighbor", "Old State House, Boston"),
        ("Boston Common", "Boston Common"),
        ("Bunker Hill", "Bunker Hill Monument"),
        ("USS Constitution", "USS Constitution Museum"),
        ("C.I.T. Ruins", "Massachusetts Institute of Technology"),
        ("The Castle", "Castle Island, Boston"),
        ("Boston Airport", "Boston Logan International Airport"),
        ("Concord", "Concord, Massachusetts"),
        ("Lexington", "Lexington, Massachusetts"),
        ("Bedford Station", "Bedford, Massachusetts"),
        ("Walden Pond", "Walden Pond"),
        ("Salem", "Salem, Massachusetts"),
        ("Lynn Woods", "Lynn Woods Reservation"),
        ("Breakheart Banks", "Breakheart Reservation"),
        ("Saugus Ironworks", "Saugus Iron Works National Historic Site"),
        ("Lake Quannapowitt", "Lake Quannapowitt"),
        ("Malden Center", "Malden, Massachusetts"),
        ("Medford Memorial Hospital", "Medford, Massachusetts"),
        ("Revere Beach Station", "Revere Beach"),
        ("Nahant Wharf", "Nahant, Massachusetts"),
        ("Spectacle Island", "Spectacle Island, Boston"),
        ("Weston Water Treatment Plant", "Weston, Massachusetts"),
        ("Lake Cochituate", "Lake Cochituate"),
        ("Chestnut Hillock Reservoir", "Chestnut Hill Reservoir"),
        ("Jamaica Plain", "Jamaica Plain, Boston"),
        ("Hyde Park", "Hyde Park, Boston"),
        ("Quincy Ruins", "Quincy, Massachusetts"),
        // Far Harbor: the Island is Mount Desert Island, Maine, its places at their real counterparts (the town of
        // Far Harbor is Bar Harbor; Acadia's observatory is on the island's central summit). Not anchored:
        // Brooke's Head Lighthouse, whose real counterpart would be a guess.
        ("Far Harbor", "Bar Harbor, Maine"),
        ("Southwest Harbor", "Southwest Harbor, Maine"),
        ("Echo Lake Lumber", "44.3197,-68.3375"),                   // Echo Lake (Wikipedia); its name geocodes elsewhere
        ("Cranberry Island Docks", "Great Cranberry Island"),
        ("National Park Visitor's Center", "Hulls Cove Visitor Center"),
        ("Acadia", "Cadillac Mountain"),
        // Nuka-World: no canon location. Fan research places it south of Springfield on the Connecticut River,
        // at the real theme park there (its map has the river with an island and a north-south interstate).
        ("Nuka-World", "Six Flags New England"),
    ]),
    (&["fallout76"], &[
        // Fallout 76: Appalachia rearranges West Virginia (it puts Grafton, Clarksburg, Philippi and Prickett's
        // Fort north of Morgantown, and its Cranberry Bog east of Harpers Ferry), so its warp is loosened until it
        // does not fold and these anchors drift a little (see `RubberSheet::new_fold_free`). Not anchored:
        // Skyline Valley (Shenandoah: Rapidan Camp, Old Rag, Stony Man), which the game adds along the south of
        // its map while the real one lies north-east of the Cranberry Glades, a move rather than drift; and lesser
        // sites the game sets against these towns, which would only add drift (Grafton itself, Monongah,
        // Philippi, Huntersville, Sugar Grove, Green Bank, Spruce Knob, Organ Cave, the New River Gorge Bridge,
        // Big Bend Tunnel, Fort Steuben, Dolly Sods, Logan).
        ("Morgantown", "Morgantown, West Virginia"),
        ("Flatwoods", "Flatwoods, West Virginia"),
        ("Point Pleasant", "Point Pleasant, West Virginia"),
        ("Welch", "Welch, West Virginia"),
        ("Beckley", "Beckley, West Virginia"),
        ("Berkeley Springs", "Berkeley Springs, West Virginia"),
        ("The Whitespring Resort", "The Greenbrier"),
        ("Cranberry Glade", "Cranberry Glades Botanical Area"),  // the Cranberry Bog
        ("Charleston Capitol Building", "West Virginia State Capitol"),
        ("Clarksburg", "Clarksburg, West Virginia"),
        ("Grafton Dam", "Tygart Lake"),
        ("Prickett's Fort", "39.5154,-80.0998"),             // Prickett's Fort State Park (Wikipedia)
        ("Harpers Ferry", "Harpers Ferry, West Virginia"),
        ("Lewisburg", "Lewisburg, West Virginia"),
        ("Summersville Dam", "Summersville Dam"),
        ("Sutton", "Sutton, West Virginia"),
        ("Helvetia", "Helvetia, West Virginia"),
        ("Seneca Rocks", "Seneca Rocks"),
        ("Tyler County Fairgrounds", "Middlebourne, West Virginia"),
        ("Athens", "Athens, Ohio"),                          // Burning Springs
        ("Albany", "Albany, Ohio"),
        // Fallout 76's expeditions: each zone at its real counterpart; zones with none (the Pitt's districts,
        // the flooded downtown, marked as the city, and its community center) at their city's centre.
        ("The Pitt - Industrial", "Pittsburgh, Pennsylvania"),
        ("The Foundry", "Pittsburgh, Pennsylvania"),
        ("The Pitt - Trench", "Pittsburgh, Pennsylvania"),
        ("Sanctum", "Pittsburgh, Pennsylvania"),
        ("Steel Tower", "U.S. Steel Tower"),
        ("The Boardwalk", "Boardwalk Hall, Atlantic City"),
        ("Showman's Pier", "Steel Pier, Atlantic City"),
        ("Aquarium of the Atlantic", "Atlantic City Aquarium"),
        ("Casino Quarter", "Caesars Atlantic City"),
        ("Atlantic City Hall", "1301 Bacharach Boulevard, Atlantic City"),
        ("Atlantic City", "Atlantic City, New Jersey"),
        ("Atlantic City Community Center", "Atlantic City, New Jersey"),
    ]),
];

/// The real places anchoring `source` (a cluster id; see `REAL_COORDS`).
pub fn real_coords(source: &str) -> impl Iterator<Item = &'static (&'static str, &'static str)> + '_ {
    REAL_COORDS.iter()
        .filter(move |(scopes, _)| scopes.iter().any(|s| source.strip_prefix(s).is_some_and(|rest| rest.is_empty() || rest.starts_with('_'))))
        .flat_map(|(_, places)| places.iter())
}

const MILES_PER_DEGREE_LAT: f32 = 69.0;

/// Hex rows (stagger on y) are sqrt(3)/2 of a column step apart, so a hex's neighbours are all
/// `miles_per_hex` away.
const HEX_ROW_FRACTION: f32 = 0.866_025_4;

/// Lambert conformal conic projection for the continental US (standard parallels 33N/45N, origin
/// 39N 96W): scale stays within ~1-2% of true across the mapped region and local shapes are kept.
mod lcc {
    const R_MILES: f64 = 3958.8;
    const PHI1: f64 = 33.0;
    const PHI2: f64 = 45.0;
    const PHI0: f64 = 39.0;
    const LAMBDA0: f64 = -96.0;
    fn t(phi_deg: f64) -> f64 { (std::f64::consts::FRAC_PI_4 + phi_deg.to_radians() / 2.0).tan() }
    fn consts() -> (f64, f64, f64) {
        let n = (PHI1.to_radians().cos() / PHI2.to_radians().cos()).ln() / (t(PHI2) / t(PHI1)).ln();
        let f = PHI1.to_radians().cos() * t(PHI1).powf(n) / n;
        (n, f, R_MILES * f / t(PHI0).powf(n))
    }
    /// (lat, lon) degrees -> (x east, y north) miles.
    pub fn forward(lat: f64, lon: f64) -> (f64, f64) {
        let (n, f, rho0) = consts();
        let rho = R_MILES * f / t(lat).powf(n);
        let theta = n * (lon - LAMBDA0).to_radians();
        (rho * theta.sin(), rho0 - rho * theta.cos())
    }
    /// (x, y) miles -> (lat, lon) degrees.
    pub fn inverse(x: f64, y: f64) -> (f64, f64) {
        let (n, f, rho0) = consts();
        let rho = n.signum() * (x * x + (rho0 - y).powi(2)).sqrt();
        let theta = (x / rho).atan2((rho0 - y) / rho);
        let lat = 2.0 * (R_MILES * f / rho).powf(1.0 / n).atan() - std::f64::consts::FRAC_PI_2;
        (lat.to_degrees(), LAMBDA0 + (theta / n).to_degrees())
    }
}

/// The output-grid convention, used by every stage: continuous (col, row) of a lat/lon on the
/// world grid at `miles_per_hex` (centre-to-centre distance between neighbouring hexes), via the
/// `lcc` projection. Rows grow south; integer indices are cell centres. (Each game's own
/// coordinate scale is separate: see `RawMapData::coordinate_scale`.)
pub fn to_grid(lat: f32, lon: f32, miles_per_hex: f32) -> (f32, f32) {
    let (x, y) = lcc::forward(lat as f64, lon as f64);
    ((x / miles_per_hex as f64) as f32, (-y / (miles_per_hex * HEX_ROW_FRACTION) as f64) as f32)
}

/// Grid position of the centre of hex (col, row). Hexes are staggered as Tiled draws them (odd rows
/// half a hex east), so a hex's six neighbours are all one hex apart. Grids start on an even row so
/// that local and world row parity agree.
pub fn hex_center(col: i32, row: i32) -> (f32, f32) {
    (col as f32 + 0.5 * row.rem_euclid(2) as f32, row as f32)
}

/// The six neighbours of hex (col, row) in the staggered layout (odd rows half a hex east).
pub fn hex_neighbours(col: i32, row: i32) -> [(i32, i32); 6] {
    let d = if row.rem_euclid(2) == 1 { 0 } else { -1 };
    [(col - 1, row), (col + 1, row), (col + d, row - 1), (col + d + 1, row - 1), (col + d, row + 1), (col + d + 1, row + 1)]
}

/// The hex whose centre is nearest grid position (col, row).
pub fn hex_at(col: f32, row: f32) -> (i32, i32) {
    let r0 = row.round() as i32;
    (r0 - 1..=r0 + 1).map(|r| {
        let c = (col - 0.5 * r.rem_euclid(2) as f32).round() as i32;
        let (hc, hr) = hex_center(c, r);
        ((c, r), (col - hc).powi(2) + ((row - hr) * HEX_ROW_FRACTION).powi(2))
    }).min_by(|a, b| a.1.total_cmp(&b.1)).unwrap().0
}

/// Inverse of `to_grid`: (lat, lon) at grid position (col, row).
pub fn from_grid(col: f32, row: f32, miles_per_hex: f32) -> (f32, f32) {
    let (lat, lon) = lcc::inverse(col as f64 * miles_per_hex as f64, -(row as f64) * (miles_per_hex * HEX_ROW_FRACTION) as f64);
    (lat as f32, lon as f32)
}

/// Vote multiplier of a terrain type when a hex picks its winner. Radiation is a hazard worth showing:
/// at 4 to 1 it takes a hex once it covers about a fifth of it, so a zone such as Camp Searchlight
/// survives on the overview scales.
pub fn salience(tile_type: &str) -> f32 {
    if tile_type == "Radiation" { 4.0 } else { 1.0 }
}

/// Terrain types that are detail over plain ground: one source's detail beats another's Sand.
const DETAILS: &[&str] = &["Water", "Grass", "Forest", "Snow", "Radiation", "City"];

/// One source's votes in a hex: its tier (`schema::tier`) and its weighted votes by type.
type SourceVotes = (u8, HashMap<String, f32>);

/// Tier of a raw map (0, the default, counts as 1).
fn tier(map: &RawMapData) -> u8 { map.priority.clamp(crate::schema::tier::MAIN, crate::schema::tier::WEAKEST) }

/// Markers of one name within this distance of each other are one place (see the merge rule in
/// `fix_translation_only`); farther apart they are different places sharing a name.
const SAME_PLACE_MILES: f32 = 25.0;

/// Whether two sightings of one name, at (lat, lon) `a` and `b`, are the same place (see `SAME_PLACE_MILES`).
pub fn same_place(a: (f32, f32), b: (f32, f32)) -> bool {
    let (dy, dx) = ((a.0 - b.0) * MILES_PER_DEGREE_LAT, (a.1 - b.1) * MILES_PER_DEGREE_LAT * a.0.to_radians().cos());
    dx.hypot(dy) <= SAME_PLACE_MILES
}

/// Marks a source's LOD land (`RawTile::lod`) as a source of its own (see `resolve_hex`).
const LOD_SOURCE: usize = 1 << (usize::BITS - 1);

/// The source a tile votes as: its own, or its LOD land's.
fn voter(source: usize, lod: bool) -> usize { if lod { source | LOD_SOURCE } else { source } }

/// The best (lowest) tier with data in a hex: its own land's, or its LOD's where it has only LOD.
fn top_tier(sources: &HashMap<usize, SourceVotes>) -> Option<u8> {
    let lod = lod_only(sources);
    sources.iter().filter(|(s, (_, votes))| (lod || *s & LOD_SOURCE == 0) && pick(votes).is_some()).map(|(_, (tier, _))| *tier).min()
}

/// Whether only LOD land voted in a hex.
fn lod_only(sources: &HashMap<usize, SourceVotes>) -> bool {
    let mut voters = sources.iter().filter(|(_, (_, votes))| pick(votes).is_some()).map(|(s, _)| s).peekable();
    voters.peek().is_some() && voters.all(|s| s & LOD_SOURCE != 0)
}

/// The winning type of a set of votes (salience-weighted; ties by name, not hash order).
fn pick(votes: &HashMap<String, f32>) -> Option<&String> {
    votes.iter().filter(|(t, _)| *t != "unknown")
        .max_by(|a, b| (a.1 * salience(a.0)).total_cmp(&(b.1 * salience(b.0))).then_with(|| b.0.cmp(a.0)))
        .map(|(t, _)| t)
}

/// Winner of a hex from each source's votes. The summed, tier-weighted vote decides, except that a
/// detail (`DETAILS`) one source gives beats a plain Sand result, unless two or more sources of a
/// higher tier than that source give Sand themselves. Among details, the highest tier wins.
fn resolve_hex(sources: &HashMap<usize, SourceVotes>) -> Option<String> {
    // LOD land (what a game shows beyond its map) is a fallback: it votes only where no source's own land does.
    if !lod_only(sources) && sources.keys().any(|s| s & LOD_SOURCE != 0) {
        let own: HashMap<usize, SourceVotes> = sources.iter().filter(|(s, _)| *s & LOD_SOURCE == 0).map(|(s, v)| (*s, v.clone())).collect();
        return resolve_hex(&own);
    }
    let mut total: HashMap<String, f32> = HashMap::new();
    for (_, votes) in sources.values() {
        for (t, w) in votes { *total.entry(t.clone()).or_default() += w; }
    }
    let winner = pick(&total)?.clone();
    if winner != "Sand" { return Some(winner); }
    let mut verdicts: Vec<(u8, f32, &String)> = sources.values()
        .filter_map(|(tier, votes)| pick(votes).map(|t| (*tier, votes[t], t))).collect();
    verdicts.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)).then_with(|| a.2.cmp(b.2)));
    for &(t, _, detail) in verdicts.iter().filter(|v| DETAILS.contains(&v.2.as_str())) {
        let sand_above = verdicts.iter().filter(|v| v.0 < t && v.2 == "Sand").count();
        if sand_above < 2 { return Some(detail.clone()); }
    }
    Some(winner)
}

/// A lake must neither vanish nor fall apart when the map is coarsened. `water` maps each 1 mi water
/// hex to the output hex it falls in. A water body (connected 1 mi water hexes) that won no output hex,
/// yet covers at least a quarter of one, takes the output hex holding most of it. A body whose output
/// hexes form separate pieces is joined along its own course: the fewest extra hexes, each holding at
/// least an eighth of a hex of it (so a coast never erodes through a peninsula).
fn keep_water_bodies(water: &HashMap<(i32, i32), (i32, i32)>, tiles: &mut HashMap<(i32, i32), String>, miles_per_hex: f32) {
    let mut keys: Vec<(i32, i32)> = water.keys().copied().collect();
    keys.sort();
    let mut seen: HashSet<(i32, i32)> = HashSet::new();
    for start in keys {
        if !seen.insert(start) { continue; }
        let mut body = vec![start];
        let mut i = 0;
        while i < body.len() {
            let (x, y) = body[i];
            i += 1;
            for n in (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy))) {
                if water.contains_key(&n) && seen.insert(n) { body.push(n); }
            }
        }
        let capacity = miles_per_hex * miles_per_hex; // 1 mi hexes per output hex
        let mut share: HashMap<(i32, i32), usize> = HashMap::new();
        for f in &body { *share.entry(water[f]).or_default() += 1; }
        let is_water = |tiles: &HashMap<(i32, i32), String>, c: &(i32, i32)| tiles.get(c).is_some_and(|t| t == "Water");
        let mut won: HashSet<(i32, i32)> = share.keys().copied().filter(|c| is_water(tiles, c)).collect();
        if won.is_empty() {
            if (body.len() as f32) < capacity / 4.0 { continue; }
            let best = *share.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))).unwrap().0;
            tiles.insert(best, "Water".to_string());
            continue;
        }
        // Gaps in the body's course: hexes it passes through (holding at least an eighth of a hex of
        // it) but lost. A gap is filled along the shortest route between two won hexes when that is
        // much shorter than going round through won hexes (or they are not joined at all): this joins
        // split rivers and keeps both arms of a ring, without fattening a lake along its shore.
        let gap: HashSet<(i32, i32)> = share.iter().filter(|(c, n)| **n as f32 >= capacity / 8.0 && !won.contains(*c)).map(|(c, _)| *c).collect();
        let bfs = |from: (i32, i32), through: &dyn Fn(&(i32, i32)) -> bool| {
            let mut dist: HashMap<(i32, i32), ((i32, i32), usize)> = HashMap::from([(from, (from, 0))]);
            let mut frontier = vec![from];
            while !frontier.is_empty() {
                let mut next = Vec::new();
                for c in frontier {
                    let d = dist[&c].1;
                    for n in hex_neighbours(c.0, c.1) {
                        if dist.contains_key(&n) { continue; }
                        let enter = through(&n);
                        if enter || won.contains(&n) { dist.insert(n, (c, d + 1)); }
                        if enter { next.push(n); }
                    }
                }
                frontier = next;
            }
            dist
        };
        let mut seen_gap: HashSet<(i32, i32)> = HashSet::new();
        let mut fill: HashSet<(i32, i32)> = HashSet::new();
        let mut gaps: Vec<(i32, i32)> = gap.iter().copied().collect();
        gaps.sort();
        for start in gaps {
            if seen_gap.contains(&start) { continue; }
            // One connected gap and the won hexes beside it.
            let mut part = vec![start];
            seen_gap.insert(start);
            let mut i = 0;
            while i < part.len() {
                let c = part[i];
                i += 1;
                for n in hex_neighbours(c.0, c.1) { if gap.contains(&n) && seen_gap.insert(n) { part.push(n); } }
            }
            let part_set: HashSet<(i32, i32)> = part.iter().copied().collect();
            let mut shore: Vec<(i32, i32)> = part.iter().flat_map(|c| hex_neighbours(c.0, c.1)).filter(|n| won.contains(n)).collect();
            shore.sort();
            shore.dedup();
            if shore.len() < 2 || shore.len() > 64 { continue; } // a dead end, or a long coast
            for (k, &a) in shore.iter().enumerate() {
                let via_gap = bfs(a, &|c| part_set.contains(c));
                let via_won = bfs(a, &|c| won.contains(c));
                for &b in &shore[k + 1..] {
                    let Some(&(_, through_gap)) = via_gap.get(&b) else { continue };
                    let around = via_won.get(&b).map_or(usize::MAX, |d| d.1);
                    if around < through_gap.saturating_add(3) { continue; }
                    let mut c = via_gap[&b].0;
                    while c != a { fill.insert(c); c = via_gap[&c].0; }
                }
            }
        }
        for c in fill { tiles.insert(c, "Water".to_string()); }
    }
}

/// Region of a tile for border drawing: every tile with data inside a real-world region. Lakes and
/// rivers lie inside the regions, so borders cross them (the Colorado is the Nevada-Arizona line);
/// the sea lies outside every region, so borders stop at the coast; no-data tiles get none.
fn tile_region(tile_type: &str, lat: f32, lon: f32) -> Option<u16> {
    if tile_type == "unknown" { return None; }
    crate::borders::region_at(lat, lon)
}

/// Key of the 1-mile bin holding (lat, lon), for neighbour lookups of manual points.
fn bin_of(lat: f32, lon: f32) -> (i32, i32) {
    let (c, r) = to_grid(lat, lon, 1.0);
    (r.round() as i32, c.round() as i32)
}


use rayon::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub min_lat: f32,
    pub max_lat: f32,
    pub min_lon: f32,
    pub max_lon: f32,
}

pub fn fix(game_clusters: Vec<RawMapData>, manual_maps: Vec<RawMapData>, miles_per_hex: Option<f32>, bounds: Option<BoundingBox>) -> FixedMapData {
    info!("Running Global Fixer on {} game clusters and {} manual maps...", game_clusters.len(), manual_maps.len());

    // If miles_per_hex is 6.0 (global phase), use Translation-Only alignment
    let resolution = miles_per_hex.unwrap_or(6.0);
    if resolution >= 5.9 && resolution <= 6.1 {
        info!("Global Phase: Using Translation-Only alignment...");
        return fix_translation_only(game_clusters, manual_maps, resolution, bounds);
    }

    // fallback for other resolutions (e.g. city maps)
    let dense_maps = game_clusters.len(); // fixed cluster grids, by contract
    let mut all_raw = game_clusters;
    all_raw.extend(manual_maps);
    
    let has_locations = all_raw.iter().any(|m| !m.locations.is_empty());
    let has_tiles = all_raw.iter().any(|m| !m.tiles.is_empty());
    
    let needs_warping = all_raw.iter().any(|m| m.coordinate_scale > 1.01 || m.coordinate_system == crate::schema::CoordinateSystem::Relative);
    
    if has_locations && has_tiles && needs_warping {
        return fix_with_terrain_inverse_warping(all_raw, dense_maps, resolution, bounds);
    }
    
    fix_legacy_pass_through(all_raw)
}

/// Placement for a Relative manual map: maps its grid (x, y) to (lat, lon) using its markers
/// that are already placed elsewhere (by name, lowercase key in `placed`) as anchors.
/// 1 anchor: translation at the map's own `miles_per_unit`. 2 anchors: similarity (uniform
/// scale + rotation) in a local miles frame. 3+: the same fold-free TPS used for games.
/// None (with a warning) when no anchor is placed yet.
fn relative_placement<'a>(map: &'a RawMapData, placed: &HashMap<String, (f32, f32)>, warnings: &mut Vec<String>)
    -> Option<Box<dyn Fn(f32, f32) -> (f32, f32) + Send + Sync + 'a>>
{
    let mut cps: Vec<ControlPoint> = map.locations.iter()
        .filter_map(|l| placed.get(&l.name.to_lowercase()).map(|&(lat, lon)| ControlPoint {
            name: l.name.clone(), game_x: l.x, game_y: l.y, real_lat: lat, real_lon: lon,
        }))
        .collect();
    cps.sort_by(|a, b| a.name.cmp(&b.name));
    cps.dedup_by(|a, b| a.game_x == b.game_x && a.game_y == b.game_y);

    let names: Vec<&str> = map.locations.iter().map(|l| l.name.as_str()).collect();
    if cps.is_empty() {
        let msg = format!("Relative map ({:?}...) has no placed anchors; skipped", &names[..names.len().min(3)]);
        warn!("{}", msg);
        warnings.push(msg);
        return None;
    }
    if cps.len() >= 3 {
        let area = (0.0, 0.0, map.width as f32, map.height as f32, 0.5);
        let (sheet, report) = RubberSheet::new_fold_free(cps, area);
        for msg in report {
            let msg = format!("Relative map {:?}: {}", &names[..names.len().min(3)], msg);
            warn!("{}", msg);
            warnings.push(msg);
        }
        return Some(Box::new(move |x, y| sheet.transform(x, y)));
    }

    // Local planar frame in miles (x east, y south), matching grid orientation.
    let lat0 = cps.iter().map(|c| c.real_lat).sum::<f32>() / cps.len() as f32;
    let kx = MILES_PER_DEGREE_LAT * lat0.to_radians().cos();
    let to_plane = move |lat: f32, lon: f32| (lon * kx, -lat * MILES_PER_DEGREE_LAT);
    let from_plane = move |e: f32, s: f32| (-s / MILES_PER_DEGREE_LAT, e / kx);
    let c0 = cps[0].clone();
    let p0 = to_plane(c0.real_lat, c0.real_lon);

    if cps.len() == 1 {
        let scale = map.miles_per_unit.unwrap_or_else(|| {
            warnings.push(format!("Relative map {:?} has one anchor and no miles_per_unit; assuming 6 mi/unit", &names[..names.len().min(3)]));
            6.0
        });
        return Some(Box::new(move |x, y| from_plane(p0.0 + (x - c0.game_x) * scale, p0.1 + (y - c0.game_y) * scale)));
    }

    // Two anchors: complex ratio a = (p1 - p0) / (g1 - g0); p = p0 + a * (g - g0).
    let c1 = &cps[1];
    let p1 = to_plane(c1.real_lat, c1.real_lon);
    let (gx, gy) = (c1.game_x - c0.game_x, c1.game_y - c0.game_y);
    let (px, py) = (p1.0 - p0.0, p1.1 - p0.1);
    let d = gx * gx + gy * gy;
    if d < 1e-6 { return Some(Box::new(move |_, _| (c0.real_lat, c0.real_lon))); }
    let (ar, ai) = ((px * gx + py * gy) / d, (py * gx - px * gy) / d);
    Some(Box::new(move |x, y| {
        let (dx, dy) = (x - c0.game_x, y - c0.game_y);
        from_plane(p0.0 + ar * dx - ai * dy, p0.1 + ai * dx + ar * dy)
    }))
}

/// New Global Aligner: Finds a single (dLat, dLon) offset per cluster based on an anchor.
/// The main landmass of the overview maps (the contiguous US, with the Mexican borderlands): a
/// cluster placed outside it is drawn as an inset instead (see `fix_translation_only`).
const MAIN_LANDMASS: BoundingBox = BoundingBox { min_lat: 24.0, max_lat: 50.0, min_lon: -125.0, max_lon: -66.0 };
/// Insets stack northwards from the south-west corner of the main map's extent (open Pacific: the
/// map's west edge is the Oregon coast, its south edge Mexico), this far apart and from the corner.
const INSET_GAP_MILES: f32 = 30.0;

pub fn fix_translation_only(game_clusters: Vec<RawMapData>, manual_maps: Vec<RawMapData>, resolution: f32, _bounds: Option<BoundingBox>) -> FixedMapData {
    info!("Aligning {} game clusters and {} manual maps via translation...", game_clusters.len(), manual_maps.len());
    
    let mut all_fixed_locations = Vec::new();
    let mut all_fixed_tiles = Vec::new();
    let mut warnings = Vec::new();
    // Insets: (cluster, region name), true positions of their markers, and the column's next free mile.
    let mut insets: Vec<(usize, String)> = Vec::new();
    let mut true_position: HashMap<String, (f32, f32)> = HashMap::new();
    let mut inset_cursor = 0.0f32;
    // Per cluster, after its alignment: markers (name, lat, lon, icon) and tiles (type, lat, lon).
    let mut placed_clusters: Vec<(Vec<(String, f32, f32, crate::schema::IconType)>, Vec<(String, f32, f32, bool)>)> = Vec::new();

    // We'll project everything to lat/lon first
    for cluster in &game_clusters {
        warnings.extend(cluster.warnings.clone());
        // 1. Pick Anchor
        // User: "pick the first marker with proper lat/lon, or the first marker if there isn't one"
        let anchor_loc = cluster.locations.iter()
            .find(|l| l.real_lat.is_some() && l.real_lon.is_some())
            .or_else(|| cluster.locations.first());

        // A cluster without markers (e.g. a mod whose markers only anchored its warp) keeps its
        // tiles where its own fix put them.
        let (d_lat, d_lon) = match anchor_loc { None => (0.0, 0.0), Some(anchor) => {
            // 2. Identify Target (Ideal) Position for Anchor
            let mut target_found = false;
            let (target_lat, target_lon) = if let (Some(la), Some(lo)) = (anchor.real_lat, anchor.real_lon) {
                let mut ideal_lat = la;
                let mut ideal_lon = lo;
                
                for (name, coords_str) in real_coords(&cluster.source) {
                    if name.eq_ignore_ascii_case(&anchor.name) {
                        if let Ok((la, lo)) = geocoding::parse_coords(coords_str) {
                            ideal_lat = la;
                            ideal_lon = lo;
                            target_found = true;
                            break;
                        }
                    }
                }
                (ideal_lat, ideal_lon)
            } else {
                let mut fallback_lat = 0.0;
                let mut fallback_lon = 0.0;
                for (name, coords_str) in real_coords(&cluster.source) {
                    if name.eq_ignore_ascii_case(&anchor.name) {
                        if let Ok((la, lo)) = geocoding::parse_coords(coords_str) {
                            fallback_lat = la;
                            fallback_lon = lo;
                            target_found = true;
                            break;
                        }
                    }
                }
                (fallback_lat, fallback_lon)
            };

            // 3. Calculate Translation Offset
            let current_lat = anchor.real_lat.unwrap_or(0.0);
            let current_lon = anchor.real_lon.unwrap_or(0.0);
            
            let (d_lat, d_lon) = if target_found {
                (target_lat - current_lat, target_lon - current_lon)
            } else {
                (0.0, 0.0) // Don't shift if not in our hardcoded California list
            };

            info!("Cluster aligned via anchor '{}' to offset (dLat: {:.4}, dLon: {:.4})", anchor.name, d_lat, d_lon);
            (d_lat, d_lon)
        } };

        let locs: Vec<_> = cluster.locations.iter().filter_map(|loc| match (loc.real_lat, loc.real_lon) {
            (Some(lat), Some(lon)) => Some((loc.name.clone(), lat + d_lat, lon + d_lon, loc.icon_type)),
            _ => { warnings.push(format!("Marker '{}' has no lat/lon after cluster fixing; not placed", loc.name)); None }
        }).collect();
        let tiles: Vec<_> = cluster.tiles.iter().filter_map(|t| Some((t.tile_type.clone(), t.real_lat? + d_lat, t.real_lon? + d_lon, t.lod))).collect();
        placed_clusters.push((locs, tiles));
    }

    // A cluster placed outside the main landmass is drawn in the inset column when the map also shows the
    // landmass, keeping its shape and scale (offsets from its centre in local miles); its markers keep
    // their true positions.
    let centres: Vec<Option<(f32, f32)>> = placed_clusters.iter().map(|(_, tiles)| {
        let land: Vec<(f32, f32)> = tiles.iter().filter(|t| t.0 != "unknown").map(|t| (t.1, t.2)).collect();
        let n = land.len() as f32;
        (!land.is_empty()).then(|| (land.iter().map(|p| p.0).sum::<f32>() / n, land.iter().map(|p| p.1).sum::<f32>() / n))
    }).collect();
    let on_landmass = |(lat, lon): (f32, f32)| (MAIN_LANDMASS.min_lat..=MAIN_LANDMASS.max_lat).contains(&lat)
        && (MAIN_LANDMASS.min_lon..=MAIN_LANDMASS.max_lon).contains(&lon);
    let shows_landmass = centres.iter().flatten().any(|&c| on_landmass(c));
    // The south-west corner of everything drawn at its real place (grid columns grow east, rows south).
    let (min_col, max_row) = placed_clusters.iter().zip(&centres).filter(|(_, c)| c.is_some_and(on_landmass))
        .flat_map(|((_, tiles), _)| tiles.iter().filter(|t| t.0 != "unknown").map(|t| to_grid(t.1, t.2, resolution)))
        .fold((f32::MAX, f32::MIN), |a, (c, r)| (a.0.min(c), a.1.max(r)));
    let corner = from_grid(min_col, max_row, resolution);
    for (cluster_idx, (locs, tiles)) in placed_clusters.into_iter().enumerate() {
        let cluster = &game_clusters[cluster_idx];
        let far = shows_landmass && centres[cluster_idx].is_some_and(|c| !on_landmass(c));
        let to_inset: Box<dyn Fn(f32, f32) -> (f32, f32)> = match centres[cluster_idx].filter(|_| far) {
            None => Box::new(|lat, lon| (lat, lon)),
            Some(centre) => {
                let kx = MILES_PER_DEGREE_LAT * centre.0.to_radians().cos();
                let miles = move |lat: f32, lon: f32| ((lon - centre.1) * kx, (lat - centre.0) * MILES_PER_DEGREE_LAT);
                let land = tiles.iter().filter(|t| t.0 != "unknown").map(|t| miles(t.1, t.2));
                let (west, north, south) = land.fold((f32::MAX, f32::MIN, f32::MAX), |a, m| (a.0.min(m.0), a.1.max(m.1), a.2.min(m.1)));
                inset_cursor += INSET_GAP_MILES;
                let slot_lat = corner.0 + (inset_cursor - south) / MILES_PER_DEGREE_LAT;
                let slot_kx = MILES_PER_DEGREE_LAT * slot_lat.to_radians().cos();
                let slot_lon = corner.1 + (INSET_GAP_MILES - west) / slot_kx;
                inset_cursor += north - south;
                let name = crate::borders::region_at(centre.0, centre.1)
                    .map_or_else(|| format!("{:.1}, {:.1}", centre.0, centre.1), |r| crate::borders::REGIONS[r as usize].name.clone());
                info!("Cluster {} lies outside the main landmass ({}): drawn as an inset", cluster_idx, name);
                insets.push((cluster_idx, name));
                Box::new(move |lat, lon| { let (e, n) = miles(lat, lon); (slot_lat + n / MILES_PER_DEGREE_LAT, slot_lon + e / slot_kx) })
            }
        };
        for (name, lat, lon, icon) in locs {
            if far { true_position.insert(name.to_lowercase(), (lat, lon)); }
            let (glat, glon) = to_inset(lat, lon);
            all_fixed_locations.push((name, glat, glon, icon, priority_weight(cluster.priority), false, false));
        }
        for (t_type, lat, lon, lod) in tiles {
            let (glat, glon) = to_inset(lat, lon);
            let (source, tier) = (voter(cluster_idx, lod), tier(cluster));
            all_fixed_tiles.push((t_type, glat, glon, priority_weight(cluster.priority), source, tier));
        }
    }

    // Process manual maps: absolute ones are projected as-is (no translation offset);
    // relative ones are placed afterwards from their markers already placed by name.
    let n_clusters = game_clusters.len(); // sources: clusters, then manual maps
    // Manual maps with at most one hex of terrain (a marker, a small circle) do not set the map's extent:
    // they are trimmed with it.
    let trimmable: HashSet<usize> = manual_maps.iter().enumerate().filter(|(_, m)| {
        let land = m.tiles.iter().filter(|t| t.tile_type != "unknown");
        if m.coordinate_system == crate::schema::CoordinateSystem::Relative { return land.count() <= 1; }
        land.filter_map(|t| Some(to_grid(t.real_lat?, t.real_lon?, resolution))).map(|(x, y)| hex_at(x, y)).collect::<HashSet<_>>().len() <= 1
    }).map(|(j, _)| n_clusters + j).collect();
    for (j, manual) in manual_maps.iter().enumerate().filter(|(_, m)| m.coordinate_system != crate::schema::CoordinateSystem::Relative) {
        let manual_idx = n_clusters + j;
        for loc in &manual.locations {
            let (Some(lat), Some(lon)) = (loc.real_lat, loc.real_lon) else {
                warnings.push(format!("Manual marker '{}' has no lat/lon; not placed", loc.name));
                continue;
            };
            all_fixed_locations.push((loc.name.clone(), lat, lon, loc.icon_type, priority_weight(manual.priority), true, trimmable.contains(&manual_idx)));
        }
        for tile in &manual.tiles {
            if let (Some(t_lat), Some(t_lon)) = (tile.real_lat, tile.real_lon) {
                let (source, tier) = (voter(manual_idx, tile.lod), tier(manual));
                all_fixed_tiles.push((tile.tile_type.clone(), t_lat, t_lon, priority_weight(manual.priority), source, tier));
            }
        }
    }
    for (j, manual) in manual_maps.iter().enumerate().filter(|(_, m)| m.coordinate_system == crate::schema::CoordinateSystem::Relative) {
        let manual_idx = n_clusters + j;
        let placed: HashMap<String, (f32, f32)> = all_fixed_locations.iter()
            .map(|(n, lat, lon, ..)| (n.to_lowercase(), (*lat, *lon))).collect();
        let Some(transform) = relative_placement(manual, &placed, &mut warnings) else { continue };
        for loc in &manual.locations {
            if placed.contains_key(&loc.name.to_lowercase()) { continue; } // anchors are already placed
            let (lat, lon) = transform(loc.x, loc.y);
            all_fixed_locations.push((loc.name.clone(), lat, lon, loc.icon_type, priority_weight(manual.priority), false, trimmable.contains(&manual_idx)));
        }
        for tile in &manual.tiles {
            let (lat, lon) = transform(tile.x as f32, tile.y as f32);
            let (source, tier) = (voter(manual_idx, tile.lod), tier(manual));
            all_fixed_tiles.push((tile.tile_type.clone(), lat, lon, priority_weight(manual.priority), source, tier));
        }
    }

    // Merge rule (as within a cluster): a place found in several sources is one marker. A manual
    // absolute position wins; otherwise the priority-weighted mean, with disagreement over 1 mi reported.
    // Markers of one name far apart are different places that share it (chain stores such as the Super
    // Duper Mart, in Washington, Boston and Appalachia): one place per group within `SAME_PLACE_MILES`.
    let miles_apart = |a: (f32, f32), b: (f32, f32)| {
        let (dy, dx) = ((a.0 - b.0) * MILES_PER_DEGREE_LAT, (a.1 - b.1) * MILES_PER_DEGREE_LAT * a.0.to_radians().cos());
        dx.hypot(dy)
    };
    type Sighting = (f32, f32, crate::schema::IconType, f32, bool, bool); // lat, lon, icon, weight, manual, trimmable
    let mut places: Vec<(String, Vec<Sighting>)> = Vec::new(); // in first-seen order
    for (name, lat, lon, icon, w, manual, trim) in all_fixed_locations {
        let near = places.iter_mut().find(|(n, ps)| n.eq_ignore_ascii_case(&name) && ps.iter().any(|p| same_place((p.0, p.1), (lat, lon))));
        match near {
            Some((_, ps)) => ps.push((lat, lon, icon, w, manual, trim)),
            None => places.push((name, vec![(lat, lon, icon, w, manual, trim)])),
        }
    }
    // (A place is trimmable only if every source of it is.)
    let mut merged: Vec<(String, f32, f32, crate::schema::IconType, bool)> = Vec::new();
    for (name, all) in places {
        let ps: Vec<_> = if all.iter().any(|p| p.4) { all.iter().filter(|p| p.4).collect() } else { all.iter().collect() };
        let wsum: f32 = ps.iter().map(|p| p.3).sum();
        let (lat, lon) = (ps.iter().map(|p| p.0 * p.3).sum::<f32>() / wsum, ps.iter().map(|p| p.1 * p.3).sum::<f32>() / wsum);
        let spread = all.iter().map(|p| miles_apart((p.0, p.1), (lat, lon))).fold(0.0, f32::max);
        if spread > 1.0 {
            let msg = format!("Sources disagree on '{}' by up to {:.1} mi from the merged position", name, spread);
            warn!("{}", msg);
            warnings.push(msg);
        }
        let top = ps.iter().max_by(|a, b| a.3.total_cmp(&b.3)).unwrap();
        merged.push((name, lat, lon, top.2, all.iter().all(|p| p.5)));
    }

    // Now convert these absolute (Lat, Lon) points into the final grid
    let mut grid_locations = Vec::new();
    let mut location_trimmable = Vec::new();

    for (name, lat, lon, icon, trim) in merged {
        location_trimmable.push(trim);
        let (gx, gy) = to_grid(lat, lon, resolution);
        let (real_lat, real_lon) = true_position.get(&name.to_lowercase()).copied().unwrap_or((lat, lon));
        grid_locations.push(FixedLocation {
            name, x: gx, y: gy, icon_type: icon,
            real_lat: Some(real_lat), real_lon: Some(real_lon),
            anchor: None, // We don't need to propagate anchors in the final grid
        });
    }

    // Downsample: each output cell takes the majority type of the finer cells landing in it
    // (ties broken by name, so the result does not depend on input order).
    let mut votes: HashMap<(i32, i32), HashMap<usize, SourceVotes>> = HashMap::new();
    let mut water: HashMap<(i32, i32), ((i32, i32), bool, u8)> = HashMap::new(); // water: 1 mi hex -> (output hex, LOD, tier)
    let mut inset_cells: HashMap<usize, (i32, i32, i32, i32)> = HashMap::new(); // cluster -> grid box
    let mut extent_cells: HashSet<(i32, i32)> = HashSet::new(); // cells a non-trimmable source has land in
    for (t_type, lat, lon, w, source, tier) in all_fixed_tiles {
        if t_type == "unknown" { continue; } // no data: must not outvote data from other sources
        let (gx, gy) = to_grid(lat, lon, resolution);
        let cell = hex_at(gx, gy);
        if !trimmable.contains(&(source & !LOD_SOURCE)) { extent_cells.insert(cell); }
        if insets.iter().any(|(c, _)| *c == source & !LOD_SOURCE) {
            let b = inset_cells.entry(source & !LOD_SOURCE).or_insert((cell.0, cell.1, cell.0, cell.1));
            *b = (b.0.min(cell.0), b.1.min(cell.1), b.2.max(cell.0), b.3.max(cell.1));
        }
        if t_type == "Water" {
            let (fx, fy) = to_grid(lat, lon, 1.0);
            let entry = water.entry(hex_at(fx, fy)).or_insert((cell, source & LOD_SOURCE != 0, tier));
            if tier < entry.2 { *entry = (cell, source & LOD_SOURCE != 0, tier); }
        }
        *votes.entry(cell).or_default().entry(source).or_insert_with(|| (tier, HashMap::new())).1.entry(t_type).or_insert(0.0) += w;
    }
    // (Water is kept only from the best tier voting in its hex, so a lower tier's sea never floods a
    // higher tier's land; LOD water only where no source's own land is.)
    let water: HashMap<(i32, i32), (i32, i32)> = water.into_iter()
        .filter(|(_, (cell, lod, tier))| (!lod || lod_only(&votes[cell])) && top_tier(&votes[cell]).is_none_or(|t| *tier <= t))
        .map(|(k, (cell, _, _))| (k, cell)).collect();
    let mut grid_tiles_map: HashMap<(i32, i32), String> = votes.into_iter()
        .filter_map(|(k, v)| resolve_hex(&v).map(|best| (k, best))).collect();
    keep_water_bodies(&water, &mut grid_tiles_map, resolution);

    // Every placed tile and every marker (a marker off the grid would be lost), and room for each
    // inset's frame (two hexes around it); trimmable sources' tiles and markers only within that (none
    // trimmable when nothing else is placed).
    let frames = || inset_cells.values().flat_map(|&(x0, y0, x1, y1)| [(x0 - 2, y0 - 2), (x1 + 2, y1 + 2)]);
    let all_trimmable = extent_cells.is_empty() && location_trimmable.iter().all(|&t| t);
    let cells = grid_tiles_map.keys().filter(|k| all_trimmable || extent_cells.contains(k)).copied()
        .chain(grid_locations.iter().zip(&location_trimmable).filter(|(_, t)| all_trimmable || !**t).map(|(l, _)| (l.x.round() as i32, l.y.round() as i32)))
        .chain(frames());
    let (min_x, max_x, min_y, max_y) = cells.fold((i32::MAX, i32::MIN, i32::MAX, i32::MIN), |(a, b, c, d), (x, y)| (a.min(x), b.max(x), c.min(y), d.max(y)));
    // Grid bounds: exactly the placed tiles and markers (nothing that does not exist is drawn).
    let (min_x, max_x, min_y, max_y) = if min_x > max_x { (0, 0, 0, 0) } else { (min_x, max_x, min_y, max_y) };
    let min_y = min_y - min_y.rem_euclid(2); // even, so local and world row parity agree

    let width = (max_x - min_x + 1) as u32;
    let height = (max_y - min_y + 1) as u32;

    let mut final_tiles = Vec::new();
    // Fill background with "unknown" or "Water" depending on context? 
    // User image shows massive purple. Let's fill the bounding box of clusters, but keep it tight.
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let t_type = grid_tiles_map.get(&(x, y)).cloned().unwrap_or_else(|| "unknown".to_string());
            let (hc, hr) = hex_center(x, y);
            let (lat, lon) = from_grid(hc, hr, resolution);
            final_tiles.push(FixedTile {
                x: x - min_x,
                y: y - min_y,
                region: tile_region(&t_type, lat, lon),
                tile_type: t_type,
                real_lat: None, real_lon: None,
                anchor: None,
                lod: false,
            });
        }
    }

    let inside = |l: &FixedLocation| (min_x..=max_x).contains(&(l.x.round() as i32)) && (min_y..=max_y).contains(&(l.y.round() as i32));
    let final_locations = grid_locations.into_iter().zip(location_trimmable).filter(|(l, t)| !t || inside(l)).map(|(mut l, _)| {
        l.x -= min_x as f32;
        l.y -= min_y as f32;
        l
    }).collect();
    let insets = insets.into_iter().filter_map(|(c, name)| inset_cells.get(&c).map(|&(x0, y0, x1, y1)|
        crate::schema::Inset { name, x0: x0 - min_x, y0: y0 - min_y, x1: x1 - min_x, y1: y1 - min_y })).collect();

    FixedMapData {
        tiles: final_tiles,
        locations: final_locations,
        width, height,
        global_offset_x: 0, global_offset_y: 0,
        warnings,
        world_bounds: game_clusters.first().and_then(|m| m.world_bounds),
        insets,
    }
}

/// Specialized fix that merges a cluster into a single RawMapData.
/// This is used for intermediate grouping of maps (e.g. FO1 + FO2).
pub fn fix_to_raw(raw_maps: Vec<RawMapData>) -> RawMapData {
    info!("Merging cluster with {} maps into raw data...", raw_maps.len());
    
    // We'll use a high-resolution intermediate fix (1.0 miles/hex)
    let resolution = 1.0;
    let source = raw_maps.first().map(|m| m.source.clone()).unwrap_or_default();
    // A merged cluster keeps the best (lowest) priority tier among its sources.
    let priority = raw_maps.iter().map(|m| m.priority.max(1)).min().unwrap_or(1);
    let fixed = fix_with_terrain_inverse_warping(raw_maps, 0, resolution, None);
    

    // We need to know the min/max indices used to derive current x,y
    // Since fix_with_terrain_inverse_warping doesn't return them, we have a problem.
    // However, if we store the reference lat/lon in FixedMapData, we can reconstruct.
    
    // Let's actually have fix_with_terrain_inverse_warping return a more detailed structure
    // or just calculate the real-world coords inside it and store them in FixedLocation.
    
    // For now, let's assume we can't easily change FixedLocation (it's for final output).
    // Let's just merge them as RawMapData but with absolute coordinates ALREADY set.
    
    // Wait, if I just want to merge FO1 and FO2, I can just rubbersheet them and return a merged RawMapData.
    
    RawMapData {
        locations: fixed.locations.into_iter().map(|l| crate::schema::RawLocation {
            name: l.name,
            x: l.x,
            y: l.y,
            icon_type: l.icon_type,
            real_lat: l.real_lat,
            real_lon: l.real_lon,
            terrain_override: None,
            anchor: l.anchor,
        }).collect(),
        tiles: fixed.tiles.into_iter().map(|t| crate::schema::RawTile {
            x: t.x,
            y: t.y,
            tile_type: t.tile_type,
            real_lat: t.real_lat,
            real_lon: t.real_lon,
            weight: 1.0,
            anchor: t.anchor,
            lod: t.lod,
        }).collect(),
        width: fixed.width,
        height: fixed.height,
        coordinate_system: crate::schema::CoordinateSystem::Absolute,
        coordinate_scale: 1.0,
        rows: None,
        has_grid: true,
        reference_lon: None,
        warnings: fixed.warnings, // Propagate warnings!
        terrain_data: None,
        world_bounds: fixed.world_bounds,
        miles_per_unit: None,
        priority,
        anchor_only_markers: false, // anchor-only markers were never placed
        control_points: Vec::new(),
        source,
    }
}

fn fix_legacy_pass_through(raw_maps: Vec<RawMapData>) -> FixedMapData {
    // Original simple path
    let mut fixed_tiles = Vec::new();
    let mut fixed_locations = Vec::new();
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for map in &raw_maps {
        for tile in &map.tiles {
            fixed_tiles.push(FixedTile {
                x: tile.x,
                y: tile.y,
                tile_type: tile.tile_type.clone(),
                real_lat: tile.real_lat,
                real_lon: tile.real_lon,
                anchor: tile.anchor.clone(),
                region: None,
                lod: tile.lod,
            });
            if (tile.x as i32) < min_x { min_x = tile.x as i32; }
            if (tile.y as i32) < min_y { min_y = tile.y as i32; }
            if (tile.x as i32) > max_x { max_x = tile.x as i32; }
            if (tile.y as i32) > max_y { max_y = tile.y as i32; }
        }
        let current_map_locations: Vec<FixedLocation> = map.locations.iter().map(|loc| {
            FixedLocation {
                name: loc.name.clone(),
                x: loc.x,
                y: loc.y,
                icon_type: loc.icon_type,
                real_lat: loc.real_lat,
                real_lon: loc.real_lon,
                anchor: loc.anchor.clone(),
            }
        }).collect();
        fixed_locations.extend(current_map_locations);
    }
    let width = if max_x >= min_x { (max_x - min_x + 1) as u32 } else { 10 };
    let height = if max_y >= min_y { (max_y - min_y + 1) as u32 } else { 10 };
    
    FixedMapData {
        tiles: fixed_tiles,
        locations: fixed_locations,
        width,
        height,
        global_offset_x: if min_x == i32::MAX { 0 } else { min_x },
        global_offset_y: if min_y == i32::MAX { 0 } else { min_y },
        warnings: raw_maps.iter().flat_map(|m| m.warnings.clone()).collect(),
        world_bounds: raw_maps.first().and_then(|m| m.world_bounds),
        insets: Vec::new(),
    }
}


// --- INVERSE WARPING PIPELINE ---
// struct MapGroup {
//     pub locations: Vec<RawMapData>, // Just the data parts? No, full map.
//     pub combined_locations: Vec<FixedLocation>,
//     pub combined_tiles: Vec<(i32, i32, String)>,
//     pub control_points: Vec<ControlPoint>,
//     pub id: usize,
// }

/// `dense_maps`: how many of `raw_maps` (the first ones) are already-fixed cluster grids.
fn fix_with_terrain_inverse_warping(raw_maps: Vec<RawMapData>, dense_maps: usize, miles_per_hex: f32, bounds_limit: Option<BoundingBox>) -> FixedMapData {
    if raw_maps.is_empty() { return FixedMapData { tiles: vec![], locations: vec![], width: 0, height: 0, global_offset_x: 0, global_offset_y: 0, warnings: vec![], world_bounds: None, insets: Vec::new() }; }
    
    let mut warnings = Vec::new();


    // 0. Build a Geographic Tile Map for all tiles that ALREADY have lat/lon (Manual data)
    // 0. Build a Manual Points Bin for sub-grid interpolation
    // We use a fixed ~6-mile grid for binning to ensure consistent lookup regardless of rendering resolution
    
    
    #[derive(Clone)]
    struct ManualPoint {
        lat: f32,
        lon: f32,
        tile_type: String,
        weight: f32,
        /// From sparse data (hand-placed manual maps, terrain overrides): may be blended into nearby
        /// hexes that have no data. Dense fixed-cluster grids never are, so nothing is invented.
        sparse: bool,
        /// The source (index in `raw_maps`; `usize::MAX` for terrain overrides) and its tier.
        source: usize,
        tier: u8,
    }
    

    let mut manual_points_bins: HashMap<(i32, i32), Vec<ManualPoint>> = HashMap::new();
    let mut manual_projected_lats = Vec::new();
    let mut manual_projected_lons = Vec::new();
    let mut all_real_world_locs: Vec<(String, f32, f32, crate::schema::IconType, Option<String>)> = Vec::new();
    let mut seen_locations: HashSet<String> = HashSet::new();

    // 1. Process Absolute manual maps (already have lat/lon); the first `dense_maps` are fixed clusters.
    for (map_idx, map) in raw_maps.iter().enumerate() {
        if map.coordinate_system == crate::schema::CoordinateSystem::Absolute && map.coordinate_scale <= 1.01 {
            for tile in &map.tiles {
                if let (Some(lat), Some(lon)) = (tile.real_lat, tile.real_lon) {
                    let (bin_y, bin_x) = bin_of(lat, lon);
                    
                    let (source, tier) = (voter(map_idx, tile.lod), tier(map));
                    manual_points_bins.entry((bin_y, bin_x)).or_default().push(ManualPoint {
                        lat, lon, tile_type: tile.tile_type.clone(),
                        weight: 2.0 * priority_weight(map.priority),
                        sparse: map_idx >= dense_maps,
                        source, tier,
                    });
                    manual_projected_lats.push(lat);
                    manual_projected_lons.push(lon);
                }
            }

            for loc in &map.locations {
                if let (Some(lat), Some(lon)) = (loc.real_lat, loc.real_lon) {
                    let name_lower = loc.name.to_lowercase();
                    if seen_locations.contains(&name_lower) { continue; }
                    seen_locations.insert(name_lower);
                    all_real_world_locs.push((loc.name.clone(), lat, lon, loc.icon_type, loc.terrain_override.clone()));
                }
            }
        }
    }

    // Build independent RubberSheets and tile maps for Interplay games (coordinate_scale > 1.0)
    // Per marker name: (display name, icon, terrain override, [(game, lat, lon)]) in first-seen order.
    let mut game_placements: HashMap<String, (String, crate::schema::IconType, Option<String>, Vec<(usize, f32, f32, f32)>)> = HashMap::new();
    let mut placement_order: Vec<String> = Vec::new();
    let mut game_sheets: Vec<Option<(RubberSheet, f32)>> = Vec::new();
    let mut game_tile_maps: Vec<HashMap<(i32, i32), (String, f32, bool)>> = Vec::new(); // (type, weight, LOD)

    for (game_idx, map) in raw_maps.iter().enumerate() {
        // Skip manual data in the rubbersheeting loop
        if map.coordinate_scale <= 1.01 { 
            game_sheets.push(None);
            game_tile_maps.push(HashMap::new());
            continue; 
        }

        let mut game_coords: HashMap<String, (f32, f32)> = HashMap::new();
        for loc in &map.locations {
            game_coords.insert(loc.name.to_lowercase(), (loc.x, loc.y));
        }

        // 1. Build control points strictly from the real places written for this source (parsed
        // coordinates, or geocoded addresses).
        let mut control_points = Vec::new();
        for (name, coords_str) in real_coords(&map.source) {
            let name_lower = name.to_lowercase();
            if let (Some((gx, gy)), Ok((lat, lon))) = (game_coords.get(&name_lower), geocoding::parse_coords(coords_str)) {
                control_points.push(ControlPoint { name: name_lower, game_x: *gx, game_y: *gy, real_lat: lat, real_lon: lon });
            }
        }

        // Extractor-derived anchors (e.g. drawn state borders).
        control_points.extend(map.control_points.iter().map(|c| ControlPoint {
            name: c.name.clone(), game_x: c.x, game_y: c.y, real_lat: c.lat, real_lon: c.lon,
        }));
        // Deterministic order (HashMap iteration is not) and no duplicate game positions.
        control_points.sort_by(|a, b| a.name.cmp(&b.name));
        control_points.dedup_by(|a, b| a.game_x == b.game_x && a.game_y == b.game_y);
        info!("Game #{} has {} control points: {:?}", game_idx, control_points.len(), control_points.iter().map(|c| &c.name).collect::<Vec<_>>());

        // A map placed by one anchor and a known scale (a fictional DLC worldspace set at a canon or
        // fan-theory location): north-up at that scale, via two synthetic anchors east and south of it.
        // With two anchors their distance gives the scale (below), known or not.
        if let (1, Some(mpu)) = (control_points.len(), map.miles_per_unit) {
            let c = control_points[0].clone();
            let d = 10_000.0f32; // game units; any distance gives the same similarity
            let (dlat, dlon) = (d * mpu / MILES_PER_DEGREE_LAT, d * mpu / (MILES_PER_DEGREE_LAT * c.real_lat.to_radians().cos()));
            control_points.truncate(1);
            control_points.push(ControlPoint { name: format!("{} (east)", c.name), game_x: c.game_x + d, game_y: c.game_y, real_lat: c.real_lat, real_lon: c.real_lon + dlon });
            control_points.push(ControlPoint { name: format!("{} (south)", c.name), game_x: c.game_x, game_y: c.game_y + d, real_lat: c.real_lat - dlat, real_lon: c.real_lon });
        } else if control_points.len() == 2 {
            // Two anchors (Fallout 1.5: Resurrection, New California): the similarity through them
            // (their distance gives the scale, their bearing the rotation), via a third point at right
            // angles to the pair on both sides. Game y grows south, so real points are taken as (east,
            // south) miles from the first anchor to keep the same handedness.
            let (a, b) = (control_points[0].clone(), control_points[1].clone());
            let kx = MILES_PER_DEGREE_LAT * a.real_lat.to_radians().cos();
            let (ve, vs) = ((b.real_lon - a.real_lon) * kx, (a.real_lat - b.real_lat) * MILES_PER_DEGREE_LAT);
            let (gx, gy) = (b.game_x - a.game_x, b.game_y - a.game_y);
            control_points.push(ControlPoint {
                name: format!("{} (right angle)", a.name),
                game_x: a.game_x - gy, game_y: a.game_y + gx,
                real_lat: a.real_lat - ve / MILES_PER_DEGREE_LAT, real_lon: a.real_lon - vs / kx,
            });
        }

        let sheet_opt = if control_points.len() >= 3 {
            let s = map.coordinate_scale;
            let area = (0.0, 0.0, map.width as f32 * s, map.height as f32 * s, s / 2.0);
            let (sheet, report) = RubberSheet::new_fold_free(control_points, area);
            for msg in report {
                let msg = format!("Game #{}: {}", game_idx, msg);
                warn!("{}", msg);
                warnings.push(msg);
            }

            // Anchors go through the warp too, so every marker sits on the terrain it came from.
            // (A map whose markers only anchor the warp places none.)
            for loc in map.locations.iter().filter(|_| !map.anchor_only_markers) {
                if loc.real_lat.is_some() && loc.real_lon.is_some() { continue; } // Handle manual markers later
                let (lat, lon) = sheet.transform(loc.x, loc.y);
                let entry = game_placements.entry(loc.name.to_lowercase()).or_insert_with(|| {
                    placement_order.push(loc.name.to_lowercase());
                    (loc.name.clone(), loc.icon_type, loc.terrain_override.clone(), Vec::new())
                });
                entry.3.push((game_idx, lat, lon, priority_weight(map.priority)));
            }
            Some((sheet, map.coordinate_scale))
        } else {
            None
        };

        game_sheets.push(sheet_opt);

        let mut tile_map: HashMap<(i32, i32), (String, f32, bool)> = HashMap::new();
        for tile in &map.tiles {
            // Speed up: intern common types to avoid excessive allocations
            let interned_type = match tile.tile_type.as_str() {
                "Grass" => "Grass",
                "Water" => "Water",
                "Sand" => "Sand",
                "Rock" => "Rock",
                "City" => "City",
                "Forest" => "Forest",
                "Mountain" => "Mountain",
                "Misc" => "Misc",
                _ => &tile.tile_type,
            };
            tile_map.insert((tile.x, tile.y), (interned_type.to_string(), tile.weight * priority_weight(map.priority), tile.lod));
        }
        game_tile_maps.push(tile_map);
    }

    // Merge rule: a place found in several games (or twice in one) gets the priority-weighted mean position;
    // disagreement beyond 1 mi is reported, never silently dropped.
    for key in &placement_order {
        if seen_locations.contains(key) { continue; } // manual absolute data takes precedence
        seen_locations.insert(key.clone());
        let (name, icon, terrain, ps) = &game_placements[key];
        let wsum = ps.iter().map(|p| p.3).sum::<f32>();
        let lat = ps.iter().map(|p| p.1 * p.3).sum::<f32>() / wsum;
        let lon = ps.iter().map(|p| p.2 * p.3).sum::<f32>() / wsum;
        let spread = ps.iter().map(|p| {
            let dy = (p.1 - lat) * MILES_PER_DEGREE_LAT;
            let dx = (p.2 - lon) * MILES_PER_DEGREE_LAT * lat.to_radians().cos();
            (dx * dx + dy * dy).sqrt()
        }).fold(0.0, f32::max);
        if spread > 1.0 {
            let msg = format!("Games disagree on '{}' by up to {:.1} mi from the merged position: {:?}", name, spread,
                ps.iter().map(|p| format!("game #{} ({:.3}, {:.3})", p.0, p.1, p.2)).collect::<Vec<_>>());
            warn!("{}", msg);
            warnings.push(msg);
        }
        all_real_world_locs.push((name.clone(), lat, lon, *icon, terrain.clone()));
    }

    // 3. Resolve Relative manual data (anchored to ANY resolved markers)
    for (map_idx, map) in raw_maps.iter().enumerate() {
        if map.coordinate_system == crate::schema::CoordinateSystem::Relative {
            // Build temporary lookup from everything resolved so far
            let mut anchor_lookup: HashMap<String, (f32, f32)> = HashMap::new();
            for (name, lat, lon, ..) in &all_real_world_locs {
                anchor_lookup.insert(name.to_lowercase(), (*lat, *lon));
            }

            let Some(transform) = relative_placement(map, &anchor_lookup, &mut warnings) else { continue };

            // Transform markers
            for loc in &map.locations {
                let name_lower = loc.name.to_lowercase();
                if seen_locations.contains(&name_lower) { continue; }
                seen_locations.insert(name_lower);

                let (lat, lon) = transform(loc.x, loc.y);
                all_real_world_locs.push((loc.name.clone(), lat, lon, loc.icon_type, loc.terrain_override.clone()));
            }

            // Transform tiles
            for tile in &map.tiles {
                let (lat, lon) = transform(tile.x as f32, tile.y as f32);
                let (bin_y, bin_x) = bin_of(lat, lon);
                
                let (source, tier) = (voter(map_idx, tile.lod), tier(map));
                manual_points_bins.entry((bin_y, bin_x)).or_default().push(ManualPoint {
                    lat, lon, tile_type: tile.tile_type.clone(),
                    weight: 2.0 * priority_weight(map.priority),
                    sparse: true,
                    source, tier,
                });
                manual_projected_lats.push(lat);
                manual_projected_lons.push(lon);
            }
        }
    }

    // Add terrain overrides as high-weight manual points
    for (_, lat, lon, _, terrain_opt) in &all_real_world_locs {
        if let Some(terrain) = terrain_opt {
            let (bin_y, bin_x) = bin_of(*lat, *lon);
            
                manual_points_bins.entry((bin_y, bin_x)).or_default().push(ManualPoint {
                    lat: *lat, 
                    lon: *lon, 
                    tile_type: terrain.clone(),
                    weight: 3.0 * priority_weight(1), // location terrain overrides come from tier-1 manual data
                    sparse: true,
                    source: usize::MAX, tier: 1,
                });
        }
    }

    // 3. Define OUTPUT GRID Bounds
    let (min_lat_raw, max_lat_raw, min_lon_raw, max_lon_raw) = if let Some(b) = bounds_limit {
        (b.min_lat, b.max_lat, b.min_lon, b.max_lon)
    } else {
        let mut projected_lats = Vec::new();
        let mut projected_lons = Vec::new();

        for (game_idx, sheet_opt) in game_sheets.iter().enumerate() {
            if let Some((sheet, terrain_scale)) = sheet_opt {
                let tiles = &raw_maps[game_idx].tiles;
                if !tiles.is_empty() {
                    let min_tx = tiles.iter().map(|t| t.x).min().unwrap() as f32;
                    let max_tx = tiles.iter().map(|t| t.x).max().unwrap() as f32;
                    let min_ty = tiles.iter().map(|t| t.y).min().unwrap() as f32;
                    let max_ty = tiles.iter().map(|t| t.y).max().unwrap() as f32;
                    
                    let corners = [
                        (min_tx * (*terrain_scale), min_ty * (*terrain_scale)),
                        (max_tx * (*terrain_scale), min_ty * (*terrain_scale)),
                        (min_tx * (*terrain_scale), max_ty * (*terrain_scale)),
                        (max_tx * (*terrain_scale), max_ty * (*terrain_scale)),
                    ];
                    
                    for (gx, gy) in corners {
                        let (lat, lon) = sheet.transform(gx, gy);
                        projected_lats.push(lat);
                        projected_lons.push(lon);
                    }
                }
            }
        }
        
        projected_lats.extend(manual_projected_lats);
        projected_lons.extend(manual_projected_lons);
        projected_lats.extend(all_real_world_locs.iter().map(|(_, lat, ..)| *lat));
        projected_lons.extend(all_real_world_locs.iter().map(|(_, _, lon, ..)| *lon));

        if projected_lats.is_empty() {
            warn!("No markers found for fixing! Returning empty map.");
            return FixedMapData {
                tiles: vec![],
                locations: vec![],
                width: 0, height: 0,
                global_offset_x: 0, global_offset_y: 0,
                warnings: warnings,
                world_bounds: None,
                insets: Vec::new(),
                    };
        }

        let buffer = 0.2;
        let min_lat = projected_lats.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
        let max_lat = projected_lats.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
        let min_lon = projected_lons.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - buffer;
        let max_lon = projected_lons.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + buffer;
        (min_lat, max_lat, min_lon, max_lon)
    };

    if min_lat_raw == 0.0 && max_lat_raw == 0.0 {
         return FixedMapData {
            tiles: vec![],
            locations: vec![],
            width: 0, height: 0,
            global_offset_x: 0, global_offset_y: 0,
            warnings: warnings,
            world_bounds: None,
            insets: Vec::new(),
            };
    }

    // North-west and south-east corners on the shared grid.
    // Grid extent of the lat/lon box: on the conic projection the extreme columns/rows can come
    // from any corner, so check all four.
    let corners = [(max_lat_raw, min_lon_raw), (max_lat_raw, max_lon_raw), (min_lat_raw, min_lon_raw), (min_lat_raw, max_lon_raw)]
        .map(|(la, lo)| to_grid(la, lo, miles_per_hex));
    let min_col = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min).floor() as i32;
    let max_col = corners.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
    let min_row = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min).floor() as i32;
    let min_row = min_row - min_row.rem_euclid(2); // even, so local and world row parity agree
    let max_row = corners.iter().map(|c| c.1).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;

    let grid_height = (max_row - min_row + 1) as u32;
    let grid_width = (max_col - min_col + 1) as u32;

    let cell_latlon = |x: u32, y: u32| {
        let (c, r) = hex_center(min_col + x as i32, min_row + y as i32);
        from_grid(c, r, miles_per_hex)
    };

    // Points with lat/lon, by the hex they fall in.
    let mut hex_points: HashMap<(i32, i32), Vec<&ManualPoint>> = HashMap::new();
    for p in manual_points_bins.values().flatten() {
        let (c, r) = to_grid(p.lat, p.lon, miles_per_hex);
        hex_points.entry(hex_at(c, r)).or_default().push(p);
    }

    // Game tiles smaller than a hex vote by area: each tile centre is warped to the hex it falls in.
    // Per game and hex: (tile count, summed tile weight per type); used where a hex holds two or more
    // of the game's tiles, otherwise the tile under the hex centre votes (below).
    // (Types are keyed with whether the tile is LOD land.)
    let game_area_votes: Vec<HashMap<(i32, i32), (usize, HashMap<(String, bool), f32>)>> = game_sheets.par_iter().enumerate().map(|(game_idx, sheet_opt)| {
        let mut bins: HashMap<(i32, i32), (usize, HashMap<(String, bool), f32>)> = HashMap::new();
        let Some((sheet, s)) = sheet_opt else { return bins };
        for (&(tx, ty), (tile_type, weight, lod)) in &game_tile_maps[game_idx] {
            let (lat, lon) = sheet.transform((tx as f32 + 0.5) * s, (ty as f32 + 0.5) * s);
            let (c, r) = to_grid(lat, lon, miles_per_hex);
            let bin = bins.entry(hex_at(c, r)).or_default();
            bin.0 += 1;
            *bin.1.entry((tile_type.clone(), *lod)).or_default() += weight;
        }
        bins
    }).collect();

    info!("Stable Grid ({:.1} mi/hex): {}x{} hexes anchored at {:?}",
             miles_per_hex, grid_width, grid_height, cell_latlon(0, 0));

    // 4. Inverse Sampling Loop (Parallelized)
    let mut fixed_tiles: Vec<FixedTile> = (0..grid_height).into_par_iter().flat_map(|y| {
        let mut row_tiles = Vec::new();
        for x in 0..grid_width {
            let (lat, lon) = cell_latlon(x, y);
            
            // Votes per source, resolved by `resolve_hex`.
            let mut by_source: HashMap<usize, SourceVotes> = HashMap::new();
            let mut vote = |source: usize, tier: u8, t: &String, w: f32| {
                *by_source.entry(source).or_insert_with(|| (tier, HashMap::new())).1.entry(t.clone()).or_insert(0.0) += w;
            };

            // A) Points with lat/lon (manual maps and already-fixed clusters). Points inside this hex share
            // its vote by area, like game tiles; only a hex with none blends nearby points.
            if let Some(ps) = hex_points.get(&(min_col + x as i32, min_row + y as i32)) {
                for p in ps {
                    vote(p.source, p.tier, &p.tile_type, 100.0 * p.weight / ps.len() as f32);
                }
            } else if !manual_points_bins.is_empty() {
                let (bin_y, bin_x) = bin_of(lat, lon);

                for by in (bin_y - 8)..=(bin_y + 8) {
                    for bx in (bin_x - 8)..=(bin_x + 8) {
                        if let Some(points) = manual_points_bins.get(&(by, bx)) {
                            for p in points.iter().filter(|p| p.sparse) {
                                // Distance in units of 6 mi.
                                let d_lat_game = (p.lat - lat) * MILES_PER_DEGREE_LAT / 6.0;
                                let d_lon_game = (p.lon - lon) * MILES_PER_DEGREE_LAT * lat.to_radians().cos() / 6.0;
                                let dist_sq = d_lat_game*d_lat_game + d_lon_game*d_lon_game;
                                if dist_sq < 4.0 {
                                    vote(p.source, p.tier, &p.tile_type, (1.0 / (dist_sq + 0.01)) * p.weight);
                                }
                            }
                        }
                    }
                }
            }

            // B) Game Data
            for (game_idx, sheet_opt) in game_sheets.iter().enumerate() {
                if let Some((sheet, terrain_scale)) = sheet_opt {
                    if let Some((n, counts)) = game_area_votes[game_idx].get(&(min_col + x as i32, min_row + y as i32)).filter(|b| b.0 >= 2) {
                        // Several tiles in this hex: they share the game's vote by area (the mean of their votes).
                        for ((tile_type, lod), w) in counts {
                            let (source, tier) = (voter(game_idx, *lod), tier(&raw_maps[game_idx]));
                            vote(source, tier, tile_type, 100.0 * w / *n as f32);
                        }
                    } else if let Some((gx, gy)) = sheet.inverse_transform(lat, lon) {
                        // Each game contributes the square that contains this point: game squares
                        // are discrete, so no blending with neighbours (that erodes narrow features).
                        // Square i covers game units [i*s, (i+1)*s).
                        let sq = ((gx / terrain_scale).floor() as i32, (gy / terrain_scale).floor() as i32);
                        if let Some((tile_type, base_weight, lod)) = game_tile_maps[game_idx].get(&sq) {
                            // Same strength a manual point has at its own location (1 / 0.01).
                            let (source, tier) = (voter(game_idx, *lod), tier(&raw_maps[game_idx]));
                            vote(source, tier, tile_type, 100.0 * base_weight);
                        }
                    }
                }
            }
            
            let best_type = resolve_hex(&by_source); // no-data ("unknown") never counts as a vote
            
            row_tiles.push(FixedTile { 
                x: x as i32, 
                y: y as i32, 
                tile_type: best_type.unwrap_or_else(|| "unknown".to_string()),
                real_lat: Some(lat),
                real_lon: Some(lon),
                anchor: None, // Inverse warping doesn't have anchors per tile
                region: None, // assigned after cleanup and trimming
                lod: lod_only(&by_source),
            });
        }
        row_tiles
    }).collect();

    let filled_count = fixed_tiles.iter().filter(|t| t.tile_type != "unknown").count();
    
    {
        // 1. Build Spatial Map for fast lookups
        let mut map_grid: HashMap<(i32, i32), (usize, String)> = HashMap::new();
        for (i, t) in fixed_tiles.iter().enumerate() {
            map_grid.insert((t.x, t.y), (i, t.tile_type.clone()));
        }

        let mut to_update: Vec<(usize, String)> = Vec::new();

        // Pass 1: Remove small Water groups (< 5)
        let mut visited_water: HashSet<(i32, i32)> = HashSet::new();
        // Identify all water tiles
        let water_keys: Vec<(i32, i32)> = map_grid.iter()
            .filter(|(_, (_, t))| t == "Water")
            .map(|(&k, _)| k)
            .collect();

        for start in water_keys {
            if visited_water.contains(&start) { continue; }
            
            // BFS
            let mut group_idxs = Vec::new();
            let mut boundary_votes: HashMap<String, i32> = HashMap::new();
            let mut queue = vec![start];
            visited_water.insert(start);
            
            if let Some((idx, _)) = map_grid.get(&start) { group_idxs.push(*idx); }

            let mut head = 0;
            while head < queue.len() {
                let (cx, cy) = queue[head];
                head += 1;

                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 { continue; }
                        let nx = cx + dx;
                        let ny = cy + dy;

                        if let Some((n_idx, n_type)) = map_grid.get(&(nx, ny)) {
                            if n_type == "Water" {
                                if !visited_water.contains(&(nx, ny)) {
                                    visited_water.insert((nx, ny));
                                    queue.push((nx, ny));
                                    group_idxs.push(*n_idx);
                                }
                            } else if n_type != "unknown" {
                                // Land neighbor -> Vote ("unknown" is no data, not land)
                                *boundary_votes.entry(n_type.clone()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }

            // Only replace when surrounded by known land (no neighbours = no evidence it is a speck).
            let best_land = boundary_votes.iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(t, _)| t.clone());
            if let (true, Some(best_land)) = (group_idxs.len() < 5, best_land) {
                for idx in group_idxs {
                    to_update.push((idx, best_land.clone()));
                }
            }
        }

        // Apply Pass 1 updates immediately so Pass 2 sees them?
        // Yes, otherwise a single land speck might be surrounded by small water specks we just removed.
        for (idx, new_type) in &to_update {
            fixed_tiles[*idx].tile_type = new_type.clone();
            // Update map_grid too for Pass 2
            let t = &fixed_tiles[*idx];
            map_grid.insert((t.x, t.y), (*idx, new_type.clone()));
        }
        to_update.clear();

        // Pass 2: Remove small Land specks (< 2)
        let mut visited_land: HashSet<(i32, i32)> = HashSet::new();
        // Identify all LAND tiles (not Water/Ocean, and not "unknown", which is no data)
        let land_keys: Vec<(i32, i32)> = map_grid.iter()
            .filter(|(_, (_, t))| t != "Water" && t != "unknown")
            .map(|(&k, _)| k)
            .collect();
            
        for start in land_keys {
            if visited_land.contains(&start) { continue; }

            // BFS
            let mut group_idxs = Vec::new();
            let mut boundary_votes: HashMap<String, i32> = HashMap::new(); // Likely mostly water
            let mut queue = vec![start];
            visited_land.insert(start);
            
            if let Some((idx, _)) = map_grid.get(&start) { group_idxs.push(*idx); }

            let mut head = 0;
            while head < queue.len() {
                let (cx, cy) = queue[head];
                head += 1;

                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 { continue; }
                        let nx = cx + dx;
                        let ny = cy + dy;

                        if let Some((n_idx, n_type)) = map_grid.get(&(nx, ny)) {
                            if n_type == "unknown" { continue; }
                            if n_type != "Water" {
                                if !visited_land.contains(&(nx, ny)) {
                                    visited_land.insert((nx, ny));
                                    queue.push((nx, ny));
                                    group_idxs.push(*n_idx);
                                }
                            } else {
                                // Water neighbor -> Vote
                                *boundary_votes.entry(n_type.clone()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }

            // Too small -> flip to the surrounding water, only if there is surrounding water.
            let best_water = boundary_votes.iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(t, _)| t.clone());
            if let (true, Some(best_water)) = (group_idxs.len() < 2, best_water) {
                for idx in group_idxs {
                    to_update.push((idx, best_water.clone()));
                }
            }
        }

        // Apply Pass 2
        for (idx, new_type) in to_update {
            fixed_tiles[idx].tile_type = new_type;
        }
    }
    
    info!("Filled {} tiles.", filled_count);

    // 5. Place Locations FIRST (on untrimmed grid)
    let mut all_locations_placed: Vec<FixedLocation> = Vec::new();
    
    for (name, lat, lon, icon_type, _) in all_real_world_locs.iter() {
        let (col, row) = to_grid(*lat, *lon, miles_per_hex);
        let grid_x = col - min_col as f32;
        let grid_y = row - min_row as f32;
        
        // Check for actual collision (markers overlapping, not just same tile)
        for existing_loc in &all_locations_placed {
            let dx = grid_x - existing_loc.x;
            let dy = grid_y - existing_loc.y;
            let distance = (dx * dx + dy * dy).sqrt();
            
            // Consider it a collision if markers are within 0.5 units
            if distance < 0.5 {
                let msg = format!("Marker collision: '{}' overlaps '{}' (distance: {:.2})", name, existing_loc.name, distance);
                warn!("WARNING: {}", msg);
                warnings.push(msg);
                break;
            }
        }
        
        all_locations_placed.push(FixedLocation {
            name: name.clone(),
            x: grid_x,
            y: grid_y,
            icon_type: *icon_type,
            real_lat: Some(*lat),
            real_lon: Some(*lon),
            anchor: None,
        });
    }

    info!("DEBUG: Placed {} markers. Terrain has {} tiles.", all_locations_placed.len(), fixed_tiles.len());

    // 6. NOW trim unknown tiles from edges
    let mut min_x_real = i32::MAX;
    let mut max_x_real = i32::MIN;
    let mut min_y_real = i32::MAX;
    let mut max_y_real = i32::MIN;
    
    // The extent of the known tiles. (A raw map's `world_bounds` are in its own game tiles, not this
    // grid's hexes, so they cannot bound it.)
    for tile in &fixed_tiles {
        if tile.tile_type != "unknown" && tile.tile_type != "" {
            if tile.x < min_x_real { min_x_real = tile.x; }
            if tile.x > max_x_real { max_x_real = tile.x; }
            if tile.y < min_y_real { min_y_real = tile.y; }
            if tile.y > max_y_real { max_y_real = tile.y; }
        }
    }

    // If no real tiles found, don't trim
    if min_x_real == i32::MAX {
        min_x_real = 0;
        max_x_real = grid_width as i32 - 1;
        min_y_real = 0;
        max_y_real = grid_height as i32 - 1;
    }

    // Add small buffer, but CLAMP to actual grid boundaries (0..grid_width)
    // to avoid creating new "unknown" borders on top/left
    let buffer_tiles = 1;
    min_x_real = (min_x_real - buffer_tiles).max(0);
    min_y_real = (min_y_real - buffer_tiles).max(0);
    max_x_real = (max_x_real + buffer_tiles).min(grid_width as i32 - 1);
    max_y_real = (max_y_real + buffer_tiles).min(grid_height as i32 - 1);
    min_y_real -= min_y_real.rem_euclid(2); // keep row parity (hex stagger)
    
    let trimmed_width = (max_x_real - min_x_real + 1) as u32;
    let trimmed_height = (max_y_real - min_y_real + 1) as u32;
    
    info!("Trimming from {}x{} to {}x{} (offset: {}, {})", 
             grid_width, grid_height, trimmed_width, trimmed_height, min_x_real, min_y_real);
    
    // Adjust tile coordinates
    let trimmed_tiles: Vec<FixedTile> = fixed_tiles.into_iter()
        .filter(|t| t.x >= min_x_real && t.x <= max_x_real && t.y >= min_y_real && t.y <= max_y_real)
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|t| FixedTile {
            x: t.x - min_x_real,
            y: t.y - min_y_real,
            region: t.real_lat.zip(t.real_lon).and_then(|(la, lo)| tile_region(&t.tile_type, la, lo)),
            tile_type: t.tile_type,
            real_lat: t.real_lat,
            real_lon: t.real_lon,
            anchor: t.anchor,
            lod: t.lod,
        })
        .collect();

    // Adjust marker coordinates
    let trimmed_locations: Vec<FixedLocation> = all_locations_placed.into_iter()
        .map(|loc| FixedLocation {
            name: loc.name,
            x: loc.x - min_x_real as f32,
            y: loc.y - min_y_real as f32,
            icon_type: loc.icon_type,
            real_lat: loc.real_lat,
            real_lon: loc.real_lon,
            anchor: loc.anchor,
        })
        .collect();

    let mut final_warnings = Vec::new();
    for map in &raw_maps {
        final_warnings.extend(map.warnings.clone());
    }
    final_warnings.extend(warnings);

    FixedMapData {
        tiles: trimmed_tiles,
        locations: trimmed_locations,
        width: trimmed_width,
        height: trimmed_height,
        global_offset_x: 0,
        global_offset_y: 0,
        warnings: final_warnings,
        world_bounds: raw_maps.first().and_then(|m| m.world_bounds),
        insets: Vec::new(),
    }
}

#[cfg(test)]
mod invariant_tests {
    //! Logical-correctness invariants for the FO1/FO2 pipeline (see plan I1–I10).
    //! Reads the cached CITY.TXT/WORLDMAP.TXT under `.cache/` via the real extractor.
    use super::*;
    use crate::extractors::Extractor;
    use crate::extractors::interplay::{FalloutInterplayExtractor, GameVariant};

    /// A lake split over several coarse hexes, winning none, keeps the one holding most of it; a
    /// pond too small for the scale does not.
    #[test]
    fn lakes_survive_coarsening() {
        // 12 mi hexes: a 40-hex lake split 22/18 over two hexes that both stay land.
        let lake: HashMap<(i32, i32), (i32, i32)> = (0..40).map(|i| ((i, 0), if i < 22 { (0, 0) } else { (1, 0) })).collect();
        let mut tiles: HashMap<(i32, i32), String> = [((0, 0), "Sand".to_string()), ((1, 0), "Sand".to_string())].into();
        keep_water_bodies(&lake, &mut tiles, 12.0);
        assert_eq!(tiles[&(0, 0)], "Water");
        assert_eq!(tiles[&(1, 0)], "Sand");
        let pond: HashMap<(i32, i32), (i32, i32)> = (0..10).map(|i| ((i, 0), (0, 0))).collect();
        let mut tiles: HashMap<(i32, i32), String> = [((0, 0), "Sand".to_string())].into();
        keep_water_bodies(&pond, &mut tiles, 12.0);
        assert_eq!(tiles[&(0, 0)], "Sand");
    }

    /// A narrow body whose output hexes fall apart is joined along its own course.
    #[test]
    fn lakes_stay_connected() {
        // 12 mi hexes, a body crossing (0,0)-(1,0)-(2,0): the ends won, the middle (30 fine hexes,
        // more than an eighth of 144) lost.
        let mut water: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
        for i in 0..100 { water.insert((i, 0), (0, 0)); }
        for i in 100..130 { water.insert((i, 0), (1, 0)); }
        for i in 130..230 { water.insert((i, 0), (2, 0)); }
        let mut tiles: HashMap<(i32, i32), String> = [((0, 0), "Water"), ((1, 0), "Sand"), ((2, 0), "Water")]
            .into_iter().map(|(k, v)| (k, v.to_string())).collect();
        keep_water_bodies(&water, &mut tiles, 12.0);
        assert_eq!(tiles[&(1, 0)], "Water");
        // A thread of 10 fine hexes (under an eighth) does not pull a hex into the lake.
        for i in 100..130 { water.remove(&(i, 0)); }
        for i in 100..110 { water.insert((i, 0), (1, 0)); }
        for i in 110..130 { water.remove(&(i, 0)); }
        for i in 110..210 { water.insert((i + 20, 0), (2, 0)); }
        let mut tiles: HashMap<(i32, i32), String> = [((0, 0), "Water"), ((1, 0), "Sand"), ((2, 0), "Water")]
            .into_iter().map(|(k, v)| (k, v.to_string())).collect();
        keep_water_bodies(&water, &mut tiles, 12.0);
        assert_eq!(tiles[&(1, 0)], "Sand");
    }

    /// Hexes at exactly `radius` steps from `centre`.
    fn hex_ring(centre: (i32, i32), radius: usize) -> Vec<(i32, i32)> {
        let mut dist: HashMap<(i32, i32), usize> = HashMap::from([(centre, 0)]);
        let mut frontier = vec![centre];
        for d in 1..=radius {
            let mut next = Vec::new();
            for c in frontier { for n in hex_neighbours(c.0, c.1) { if !dist.contains_key(&n) { dist.insert(n, d); next.push(n); } } }
            frontier = next;
        }
        let mut ring: Vec<_> = dist.into_iter().filter(|&(_, d)| d == radius).map(|(c, _)| c).collect();
        ring.sort();
        ring
    }

    /// One water body covering `cells` with the given fine-hex counts (one contiguous fine row).
    fn body(cells: &[((i32, i32), usize)]) -> HashMap<(i32, i32), (i32, i32)> {
        let mut water = HashMap::new();
        let mut i = 0;
        for &(c, n) in cells { for _ in 0..n { water.insert((i, 0), c); i += 1; } }
        water
    }

    /// Both arms of a ring survive: a lost gap in one arm is refilled. A lake's shore, where it
    /// holds a minority of the hexes around, is not.
    #[test]
    fn lake_shapes_persist() {
        let ring = hex_ring((5, 6), 2);
        let gap = [ring[0], *ring.iter().find(|c| hex_neighbours(ring[0].0, ring[0].1).contains(c)).unwrap()];
        let cells: Vec<_> = ring.iter().map(|&c| (c, if gap.contains(&c) { 30 } else { 100 })).collect();
        let mut tiles: HashMap<(i32, i32), String> = ring.iter().map(|&c| (c, if gap.contains(&c) { "Sand" } else { "Water" }.to_string())).collect();
        keep_water_bodies(&body(&cells), &mut tiles, 12.0);
        assert!(gap.iter().all(|c| tiles[c] == "Water"), "ring gap not refilled");

        let lake: Vec<(i32, i32)> = std::iter::once((5, 6)).chain(hex_ring((5, 6), 1)).collect();
        let shore = hex_ring((5, 6), 2);
        let cells: Vec<_> = lake.iter().map(|&c| (c, 144)).chain(shore.iter().map(|&c| (c, 40))).collect();
        let mut tiles: HashMap<(i32, i32), String> = lake.iter().map(|&c| (c, "Water".to_string()))
            .chain(shore.iter().map(|&c| (c, "Sand".to_string()))).collect();
        keep_water_bodies(&body(&cells), &mut tiles, 12.0);
        assert!(shore.iter().all(|c| tiles[c] == "Sand"), "shore was fattened");
    }

    /// The detail-over-Sand rule: a source's detail beats another's Sand, but not two higher-tier
    /// sources agreeing on Sand, nor a higher-tier non-Sand type.
    #[test]
    fn details_beat_plain_ground() {
        let src = |tier: u8, t: &str, w: f32| -> SourceVotes { (tier, [(t.to_string(), w)].into()) };
        let hex = |list: Vec<SourceVotes>| -> HashMap<usize, SourceVotes> { list.into_iter().enumerate().collect() };
        // One tier-1 Sand (weight 16) against a tier-5 Water (weight 1): the detail wins.
        assert_eq!(resolve_hex(&hex(vec![src(1, "Sand", 16.0), src(5, "Water", 1.0)])).as_deref(), Some("Water"));
        // Two tier-1 sources agree on Sand: consensus holds.
        assert_eq!(resolve_hex(&hex(vec![src(1, "Sand", 16.0), src(1, "Sand", 16.0), src(5, "Water", 1.0)])).as_deref(), Some("Sand"));
        // A higher-tier Mountain is not plain ground: the weighted vote stands.
        assert_eq!(resolve_hex(&hex(vec![src(1, "Mountain", 16.0), src(5, "Grass", 1.0)])).as_deref(), Some("Mountain"));
        // Competing details: the higher tier's wins.
        assert_eq!(resolve_hex(&hex(vec![src(1, "Sand", 16.0), src(3, "City", 4.0), src(5, "Water", 1.0)])).as_deref(), Some("City"));
        // Same-tier detail against a single Sand also counts ("every source pair").
        assert_eq!(resolve_hex(&hex(vec![src(1, "Sand", 16.0), src(1, "Water", 4.0)])).as_deref(), Some("Water"));
        // No data is not a vote.
        assert_eq!(resolve_hex(&hex(vec![src(1, "unknown", 16.0)])), None);
    }

    /// LOD land is a fallback: a tier-1 game's LOD detail loses to a mod's own Sand, but decides a hex
    /// no source's own land reaches (and one whose own source has no data).
    #[test]
    fn lod_land_is_a_fallback() {
        let src = |tier: u8, t: &str, w: f32| -> SourceVotes { (tier, [(t.to_string(), w)].into()) };
        let lod_water = (voter(0, true), src(1, "Water", 16.0));
        let hex = |list: Vec<(usize, SourceVotes)>| -> HashMap<usize, SourceVotes> { list.into_iter().collect() };
        assert_eq!(resolve_hex(&hex(vec![lod_water.clone(), (voter(1, false), src(6, "Sand", 1.0))])).as_deref(), Some("Sand"));
        assert_eq!(resolve_hex(&hex(vec![lod_water.clone(), (voter(1, true), src(6, "Sand", 1.0))])).as_deref(), Some("Water"));
        assert_eq!(resolve_hex(&hex(vec![lod_water.clone(), (voter(1, false), src(6, "unknown", 1.0))])).as_deref(), Some("Water"));
        assert!(lod_only(&hex(vec![lod_water])));
    }

    /// Hex centres follow the staggered layout: all six neighbours are one hex apart, and every
    /// centre (and any point nearer to it than to other centres) maps back to its hex.
    #[test]
    fn hex_grid_is_staggered() {
        for (c, r) in [(0, 0), (3, 1), (-2, -1), (5, 6)] {
            let (x, y) = hex_center(c, r);
            assert_eq!(hex_at(x, y), (c, r));
            assert_eq!(hex_at(x + 0.3, y - 0.3), (c, r));
            let d = if r.rem_euclid(2) == 1 { 0 } else { -1 };
            for n in [(c - 1, r), (c + 1, r), (c + d, r - 1), (c + d + 1, r - 1), (c + d, r + 1), (c + d + 1, r + 1)] {
                let (nx, ny) = hex_center(n.0, n.1);
                let dist = (nx - x).hypot((ny - y) * HEX_ROW_FRACTION);
                assert!((dist - 1.0).abs() < 1e-5, "{:?} -> {:?}: {}", (c, r), n, dist);
            }
        }
    }

    fn game(v: GameVariant) -> RawMapData { FalloutInterplayExtractor::new(v).extract() }

    fn anchors() -> HashMap<String, (f32, f32)> {
        real_coords("fallout_interplay")
            .map(|(n, c)| (n.to_lowercase(), geocoding::parse_coords(c).unwrap()))
            .collect()
    }

    fn control_points(map: &RawMapData, skip: Option<&str>) -> Vec<ControlPoint> {
        let a = anchors();
        map.locations.iter()
            .filter(|l| Some(l.name.to_lowercase().as_str()) != skip)
            .filter_map(|l| a.get(&l.name.to_lowercase()).map(|&(lat, lon)| ControlPoint {
                name: l.name.clone(), game_x: l.x, game_y: l.y, real_lat: lat, real_lon: lon,
            }))
            .collect()
    }

    /// Great-circle-free local distance in true miles (cos(lat) scaling, not the 35N constant).
    fn miles(a: (f32, f32), b: (f32, f32)) -> f32 {
        let lat_m = (a.0 + b.0) / 2.0;
        let dy = (a.0 - b.0) * MILES_PER_DEGREE_LAT;
        let dx = (a.1 - b.1) * MILES_PER_DEGREE_LAT * lat_m.to_radians().cos();
        (dx * dx + dy * dy).sqrt()
    }

    #[test]
    fn report_transform_quality() {
        for v in [GameVariant::Fallout1, GameVariant::Fallout2] {
            let map = game(v);
            // The world map is 28x30 squares of 50 px, whatever tile resolution the extractor emits.
            assert_eq!((map.width as f32 * map.coordinate_scale, map.height as f32 * map.coordinate_scale), (1400.0, 1500.0),
                "{:?}: world map must span 1400x1500 px", v);
            let cps = control_points(&map, None);
            let (sheet, report) = RubberSheet::new_fold_free(cps.clone(), (0.0, 0.0, 28.0 * 50.0, 30.0 * 50.0, 25.0));
            for line in report { println!("{:?} {}", v, line); }

            // I3: exact at anchors
            for cp in &cps {
                let p = sheet.transform(cp.game_x, cp.game_y);
                println!("{:?} anchor-exact {:<24} err {:.3} mi", v, cp.name, miles(p, (cp.real_lat, cp.real_lon)));
            }

            // I8: leave-one-out
            for cp in &cps {
                let loo = RubberSheet::new(control_points(&map, Some(&cp.name.to_lowercase())));
                let p = loo.transform(cp.game_x, cp.game_y);
                println!("{:?} LOO {:<24} err {:6.1} mi  predicted ({:.3}, {:.3}) anchor ({:.3}, {:.3})  dN {:+.1} mi dE {:+.1} mi",
                    v, cp.name, miles(p, (cp.real_lat, cp.real_lon)), p.0, p.1, cp.real_lat, cp.real_lon,
                    (p.0 - cp.real_lat) * MILES_PER_DEGREE_LAT,
                    (p.1 - cp.real_lon) * MILES_PER_DEGREE_LAT * cp.real_lat.to_radians().cos());
            }

            // I2: round trip forward→inverse over the whole world grid, in squares
            let mut worst_rt = 0f32;
            // I3: fold check — sign of Jacobian determinant over the grid
            let (mut pos, mut neg) = (0, 0);
            for sy in 0..30 { for sx in 0..28 {
                let (gx, gy) = ((sx as f32 + 0.5) * 50.0, (sy as f32 + 0.5) * 50.0);
                let p = sheet.transform(gx, gy);
                let (bx, by) = sheet.inverse_transform(p.0, p.1).unwrap();
                worst_rt = worst_rt.max(((bx - gx).powi(2) + (by - gy).powi(2)).sqrt() / 50.0);
                let h = 5.0;
                let px = sheet.transform(gx + h, gy);
                let py = sheet.transform(gx, gy + h);
                let det = (px.0 - p.0) * (py.1 - p.1) - (px.1 - p.1) * (py.0 - p.0);
                if det > 0.0 { pos += 1 } else { neg += 1; println!("{:?} fold at square ({}, {})", v, sx, sy) }
            }}
            println!("{:?} round-trip worst {:.2} squares; jacobian +{} / -{}", v, worst_rt, pos, neg);
            assert_eq!(neg, 0, "{:?}: warp folds (I3)", v);
            assert!(worst_rt < 0.01, "{:?}: inverse is not the forward's inverse (I2)", v);

            // I4: implied miles per square (affine scale), x and y separately
            let c = (14.0 * 50.0, 15.0 * 50.0);
            let o = sheet.transform(c.0, c.1);
            println!("{:?} miles/square ~ x {:.1}, y {:.1}", v,
                miles(o, sheet.transform(c.0 + 50.0, c.1)), miles(o, sheet.transform(c.0, c.1 + 50.0)));
        }
    }

    /// Markers and terrain come from the same warp, so each game marker must sit on the game
    /// square it was drawn on. Checks the warped tile under each marker against the source square.
    #[test]
    fn markers_sit_on_their_source_square() {
        let maps = vec![game(GameVariant::Fallout1), game(GameVariant::Fallout2)];
        let fixed = fix_with_terrain_inverse_warping(maps.clone(), 0, 1.0, None);
        let tile_at: HashMap<(i32, i32), &str> = fixed.tiles.iter().map(|t| ((t.x, t.y), t.tile_type.as_str())).collect();
        let (mut ok, mut total) = (0, 0);
        for map in &maps {
            let src: HashMap<(i32, i32), &str> = map.tiles.iter().map(|t| ((t.x, t.y), t.tile_type.as_str())).collect();
            for loc in &map.locations {
                let s = map.coordinate_scale;
                let want = src[&((loc.x / s).floor() as i32, (loc.y / s).floor() as i32)];
                let Some(f) = fixed.locations.iter().find(|f| f.name == loc.name) else { continue };
                let got = tile_at.get(&hex_at(f.x, f.y)).copied().unwrap_or("none");
                total += 1;
                if got == want { ok += 1 } else { println!("{:<28} source {:<8} warped {}", loc.name, want, got) }
            }
        }
        println!("{}/{} markers on a tile of their source square's type", ok, total);
        assert!(ok * 10 >= total * 9, "markers drift off their terrain (I1/I2)");
    }

    /// Relative manual maps must survive the global (translation-only) pass: markers that are
    /// not anchors get placed from the anchors, never dropped.
    #[test]
    fn relative_manual_maps_are_placed_globally() {
        let mk = |name: &str, x: f32, y: f32, lat: Option<f32>, lon: Option<f32>| crate::schema::RawLocation {
            name: name.into(), x, y, icon_type: crate::schema::IconType::City,
            real_lat: lat, real_lon: lon, terrain_override: None, anchor: None,
        };
        let cluster = RawMapData {
            locations: vec![mk("A", 0.0, 0.0, Some(37.0), Some(-120.0)), mk("B", 0.0, 0.0, Some(38.0), Some(-120.0)),
                            mk("C", 0.0, 0.0, Some(37.0), Some(-119.0))],
            coordinate_scale: 1.0, ..Default::default()
        };
        let relative = RawMapData {
            locations: vec![mk("A", 0.0, 10.0, None, None), mk("B", 0.0, 0.0, None, None),
                            mk("C", 10.0, 10.0, None, None), mk("Mid", 5.0, 5.0, None, None)],
            coordinate_system: crate::schema::CoordinateSystem::Relative,
            width: 11, height: 11, coordinate_scale: 1.0, miles_per_unit: Some(6.0), ..Default::default()
        };
        let fixed = fix_translation_only(vec![cluster], vec![relative], 6.0, None);
        let mid = fixed.locations.iter().find(|l| l.name == "Mid").expect("non-anchor marker of a relative map was dropped");
        // 3 anchors = exact affine; Mid is the midpoint of B and C, i.e. (37.5, -119.5).
        assert!((mid.real_lat.unwrap() - 37.5).abs() < 1e-3 && (mid.real_lon.unwrap() + 119.5).abs() < 1e-3,
            "Mid placed at {:?}, {:?}", mid.real_lat, mid.real_lon);
    }

    /// Every real point the warp covers must invert (else the cell gets no terrain: a hole).
    /// Probes off-centre points on a fine grid, not just square centres.
    #[test]
    fn inverse_has_no_holes() {
        for v in [GameVariant::Fallout1, GameVariant::Fallout2] {
            let map = game(v);
            let (sheet, _) = RubberSheet::new_fold_free(control_points(&map, None), (0.0, 0.0, 1400.0, 1500.0, 25.0));
            let mut fails = 0;
            for gy in (3..1500).step_by(7) { for gx in (3..1400).step_by(7) {
                let p = sheet.transform(gx as f32, gy as f32);
                match sheet.inverse_transform(p.0, p.1) {
                    Some((bx, by)) if (bx - gx as f32).abs() < 0.5 && (by - gy as f32).abs() < 0.5 => {}
                    _ => fails += 1,
                }
            }}
            assert_eq!(fails, 0, "{:?}: {} points do not invert", v, fails);
        }
    }

    /// Votes are weighted by source priority: where a tier-1 and a tier-3 source disagree on a
    /// cell (neither being a detail over plain Sand), the tier-1 type wins even when outnumbered
    /// (weights 16 vs 2 x 4).
    #[test]
    fn higher_priority_source_wins_the_vote() {
        let src = |priority: u8, tiles: Vec<(&str, f32)>| RawMapData {
            locations: vec![crate::schema::RawLocation { name: format!("anchor{}", priority), x: 0.0, y: 0.0,
                icon_type: crate::schema::IconType::Other, real_lat: Some(37.0), real_lon: Some(-118.0),
                terrain_override: None, anchor: None }],
            tiles: tiles.into_iter().map(|(t, dlon)| crate::schema::RawTile { x: 0, y: 0, tile_type: t.into(),
                real_lat: Some(37.0), real_lon: Some(-118.0 + dlon), weight: 1.0, anchor: None, lod: false }).collect(),
            coordinate_scale: 1.0, priority, ..Default::default()
        };
        let fixed = fix_translation_only(
            vec![src(1, vec![("Mountain", 0.0)]), src(3, vec![("Rock", 0.0), ("Rock", 0.001)])], vec![], 6.0, None);
        let land: Vec<&str> = fixed.tiles.iter().map(|t| t.tile_type.as_str()).filter(|t| *t != "unknown").collect();
        assert_eq!(land, vec!["Mountain"]);
    }

    /// A manual map with no terrain (a lone marker) does not stretch the map: it is trimmed with it,
    /// while one inside the land stays.
    #[test]
    fn lone_manual_markers_are_trimmed() {
        let marker = |name: &str, lat: f32, lon: f32| crate::schema::RawLocation { name: name.into(), x: 0.0, y: 0.0,
            icon_type: crate::schema::IconType::City, real_lat: Some(lat), real_lon: Some(lon), terrain_override: None, anchor: None };
        let tile = |dlon: f32| crate::schema::RawTile { x: 0, y: 0, tile_type: "Sand".into(), real_lat: Some(37.0),
            real_lon: Some(-118.0 + dlon), weight: 1.0, anchor: None, lod: false };
        let cluster = RawMapData { locations: vec![marker("Anchor", 37.0, -118.0)], tiles: (0..20).map(|i| tile(i as f32 * 0.1)).collect(),
            coordinate_scale: 1.0, priority: 1, ..Default::default() };
        let lone = |name: &str, lat: f32, lon: f32| RawMapData { locations: vec![marker(name, lat, lon)], coordinate_scale: 1.0,
            coordinate_system: crate::schema::CoordinateSystem::Absolute, ..Default::default() };
        let fixed = fix_translation_only(vec![cluster], vec![lone("Far", 47.6, -122.3), lone("Near", 37.0, -117.5)], 6.0, None);
        let names: Vec<&str> = fixed.locations.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"Near") && !names.contains(&"Far"), "{:?}", names);
        assert!(fixed.height < 10, "the far marker must not stretch the map: height {}", fixed.height);
    }

    /// A lower tier's water is not forced over a higher tier's land: a mod's lake covering a third of a
    /// hex, outvoted there by a main game's town, leaves the town.
    #[test]
    fn lower_tier_water_does_not_flood_higher_tier_land() {
        let (lat, lon) = from_grid(hex_center(100, 100).0, hex_center(100, 100).1, 6.0);
        let marker = |name: &str| crate::schema::RawLocation { name: name.into(), x: 0.0, y: 0.0, icon_type: crate::schema::IconType::City,
            real_lat: Some(lat), real_lon: Some(lon), terrain_override: None, anchor: None };
        let tile = |t: &str, dlat: f32, dlon: f32| crate::schema::RawTile { x: 0, y: 0, tile_type: t.into(), real_lat: Some(lat + dlat),
            real_lon: Some(lon + dlon), weight: 1.0, anchor: None, lod: false };
        let town = RawMapData { locations: vec![marker("Town")], tiles: vec![tile("City", 0.0, 0.0)], coordinate_scale: 1.0, priority: 1, ..Default::default() };
        // A lake of 10 one-mile hexes (weight 1 each against the town's 16) within 2 miles of the centre.
        let lake: Vec<_> = (0..10).map(|i| tile("Water", (i % 3) as f32 * 0.012 - 0.012, (i / 3) as f32 * 0.016 - 0.024)).collect();
        let mod_map = RawMapData { locations: vec![marker("Lake")], tiles: lake, coordinate_scale: 1.0, priority: crate::schema::tier::MODS, ..Default::default() };
        let fixed = fix_translation_only(vec![town, mod_map], vec![], 6.0, None);
        let land: Vec<&str> = fixed.tiles.iter().map(|t| t.tile_type.as_str()).filter(|t| *t != "unknown").collect();
        assert_eq!(land, vec!["City"], "{:?}", land);
    }

    /// Two anchors and no known scale place a map by the similarity through them: a marker at right
    /// angles to the pair (same distance, turned the way game "south" is) lands at the matching real
    /// spot. (Albuquerque and Rat Hole, Resurrection's anchors: Rat Hole is up and to the left.)
    #[test]
    fn two_anchors_give_a_similarity() {
        let loc = |name: &str, x: f32, y: f32| crate::schema::RawLocation { name: name.into(), x, y,
            icon_type: crate::schema::IconType::City, real_lat: None, real_lon: None, terrain_override: None, anchor: None };
        let tiles = (0..40).flat_map(|y| (0..40).map(move |x| crate::schema::RawTile { x, y, tile_type: "Sand".into(),
            real_lat: None, real_lon: None, weight: 1.0, anchor: None, lod: false })).collect();
        // Game units: Albuquerque at (300, 300), Rat Hole up-left; the probe at right angles (down-left).
        let map = RawMapData { locations: vec![loc("Albuquerque", 300.0, 300.0), loc("Rat Hole", 150.0, 150.0), loc("Probe", 150.0, 450.0)],
            tiles, width: 40, height: 40, coordinate_scale: 10.0, priority: 1, source: "mod_fallout2_fallout_resurrection".into(), ..Default::default() };
        let fixed = fix_with_terrain_inverse_warping(vec![map], 0, 1.0, None);
        let at = |n: &str| fixed.locations.iter().find(|l| l.name == n).and_then(|l| l.real_lat.zip(l.real_lon)).unwrap();
        let ((la, lo), (lr, lor), (lp, lop)) = (at("Albuquerque"), at("Rat Hole"), at("Probe"));
        let kx = MILES_PER_DEGREE_LAT * la.to_radians().cos();
        let miles = |lat: f32, lon: f32| ((lon - lo) * kx, (lat - la) * MILES_PER_DEGREE_LAT); // (east, north)
        let (r, p) = (miles(lr, lor), miles(lp, lop));
        // The probe is the anchor pair's vector turned 90 degrees: same length, perpendicular.
        assert!((r.0.hypot(r.1) - p.0.hypot(p.1)).abs() < 3.0, "lengths {:?} {:?}", r, p);
        assert!((r.0 * p.0 + r.1 * p.1).abs() < 3.0 * r.0.hypot(r.1), "not perpendicular {:?} {:?}", r, p);
        // Rat Hole north-west of Albuquerque puts the probe (down-left in the game) south-west.
        assert!(p.0 < 0.0 && p.1 < 0.0, "probe should be south-west: {:?}", p);
    }

    /// Map scale: distances measured on the hex grid (column step = 1 hex, row step = sqrt(3)/2)
    /// match real great-circle distances across the whole mapped region.
    #[test]
    fn grid_distances_match_real_distances() {
        let haversine = |a: (f32, f32), b: (f32, f32)| {
            let (la1, la2) = (a.0.to_radians() as f64, b.0.to_radians() as f64);
            let (dla, dlo) = (la2 - la1, (b.1 - a.1).to_radians() as f64);
            let h = (dla / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlo / 2.0).sin().powi(2);
            2.0 * 3958.8 * h.sqrt().asin()
        };
        let grid_miles = |a: (f32, f32), b: (f32, f32)| {
            let (pa, pb) = (to_grid(a.0, a.1, 6.0), to_grid(b.0, b.1, 6.0));
            (((pb.0 - pa.0) * 6.0).powi(2) + ((pb.1 - pa.1) * 6.0 * HEX_ROW_FRACTION).powi(2)).sqrt() as f64
        };
        let places = [("San Francisco", (37.79, -122.42)), ("Los Angeles", (34.05, -118.24)), ("Denver", (39.74, -104.99)),
                      ("Salt Lake City", (40.76, -111.89)), ("St. Louis", (38.63, -90.20)), ("Chicago", (41.88, -87.63))];
        for (i, (na, a)) in places.iter().enumerate() {
            for (nb, b) in &places[i + 1..] {
                let (real, map) = (haversine(*a, *b), grid_miles(*a, *b));
                let err = (map - real) / real;
                println!("{:<14} - {:<14} real {:6.0} mi  map {:6.0} mi  {:+.1}%", na, nb, real, map, err * 100.0);
                assert!(err.abs() < 0.01, "{} - {}: {:+.1}%", na, nb, err * 100.0);
            }
        }
    }
}
