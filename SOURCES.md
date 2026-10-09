# Sources checklist

Every Fallout game and add-on, and whether the map uses it. Tiers (vote weight and build order;
`schema::tier`): 1 main games and manual locations, 2 Fallout 76, 3 simulations of the past
(Operation Anchorage), 4 Tactics and Van Buren, 5 Brotherhood of Steel, the Fallout Bible and Exodus, 6 mods. Vote weights
16/8/6/4/2/1. A game's LOD land (what it shows beyond its own map) only fills where no source's own
land is.

Places far from the contiguous US (Alaska) are drawn as framed insets in the overviews' south-west
corner; their own cluster maps are at the real place.

✅ on the map · ⬜ to do · ⏸ deferred · ➖ nothing to place (no geography, or an interior/simulation)

## Fallout (1997) — tier 1
- ✅ Southern California world map (`interplay.rs`; with Fallout 2 in the `fallout_interplay` cluster)

## Fallout 2 (1998) — tier 1
- ✅ Northern California / Oregon / Nevada world map (`interplay.rs`)

## Fallout Tactics: Brotherhood of Steel (2001) — tier 4
- ✅ Midwest world map, Illinois to Colorado (`tactics.rs`)

## Van Buren (cancelled; 2007 tech demo) — tier 4
- ✅ Colorado / Utah / Arizona world map (`van_buren.rs`); Denver shown as Dog City
- ✅ Design documents (leaked to No Mutants Allowed 2005–06; Internet Archive
  `van-buren-docs-and-info`): Tibbets Prison, Denver, Boulder, Blackfoot & Hangdogs, Twin Mothers,
  Ouroboros, Mesa Verde, the Nursery, Maxson Bunker, New Canaan, Jericho, Burham Springs, Hoover Dam,
  the Reservation, Bloomfield Space Center, B.O.M.B.-001, Fort Abandon; the companion documents (CRB-S,
  Mr. Handy, Devil Dog, Kurisu, Beatrice, Otto Steed, Measles, Kyle, Bear, Sid, Alexandra); the
  segregated game text; the Circle Junction document (Nukapedia). Places in `input/manual/van_buren`
  (tier 4); those a released game also mentions go to `input/manual` at its tier (New Canaan)

## Fallout: Brotherhood of Steel (2004, PS2/Xbox) — tier 5
- ⏸ Carbon, Los (Dallas), the Vault — deferred: needs the engine's mesh/material format
  (`.claude/plans/fallout_bos.md`)
- ➖ Fallout: Brotherhood of Steel 2 — cancelled
- ✅ Fallout: Brotherhood of Steel 2 design document (leaked 2009; fallout.wiki): its Texas (Lubbock, Abilene's
  Lone Star and Vault 39, Fort Worth, the Corpse at Corpus Christi) in `input/manual/design_docs`

## Fallout Bible (2002–2003, Chris Avellone) — tier 5
- ➖ Interplay's setting notes (Duck and Cover PDFs, Internet Archive `falloutbiblecomplete`): Q&A on
  Fallout 1 and 2, whose places the games already map, so no new places. Its value is canon on which
  real places they are (Bible 6, 8): Necropolis = Bakersfield (the holodisc's Vault 12; the game draws
  it near Baker), Klamath = Klamath Falls, Redding = Redding, Modoc after the Modoc National Forest;
  the Hub, Junktown, Gecko and Arroyo are made up. Manual JSON may set `"tier"` for such sources.

## Design documents — tier 5 (Fallout 1/2, New Vegas, Brotherhood of Steel 2), 4 (Van Buren, Fallout Extreme)
Transcriptions on Nukapedia (fallout.fandom.com "Design document"); places a released game also
mentions go to `input/manual` at that game's tier.
- ✅ Fallout: vision statement; Vault 13: A GURPS Post-Nuclear Adventure timeline; Burrows, Mr. Handy
  and Vipers design documents
- ✅ Fallout 2: EPA design document; Kaga encounter notes
- ✅ Fallout: New Vegas: Dead Money and Lonesome Road design documents
- ✅ Fallout Extreme (cancelled): game proposal, Xbox concept submission (and draft), 'X' test world
  design document — places outside the US out of scope
- Van Buren and Brotherhood of Steel 2: in their sections above

## Mentioned-only locations (fallout.wiki) — the mentioning game's tier
- ✅ The wiki's "mentioned-only locations" categories (654 pages: Fallout 1, 2, 3 and its add-ons, New
  Vegas and its add-ons, 4 and Far Harbor, 76, Tactics, the Bible, Brotherhood of Steel, the RPG,
  Wasteland Warfare): places the games name but never show. Only major cities, major landmarks, vaults
  and places the text locates or describes; markers in `input/manual`

## Fallout 3 (2008) — tier 1 (Mutagen)
- ✅ Capital Wasteland (`Wasteland`), incl. its LOD ring; DC's ruins contribute their map markers
- ✅ Operation: Anchorage — the simulated 2077 Anchorage front (tier 3): the battle, the Artillery
  Overlook and the Chinese HQ, at Anchorage, Alaska; the Glacier (the opening cliff climb, other land)
  is left out
- ✅ The Pitt — Pittsburgh (the Pitt and its three districts, which share its frame; at Downtown
  Pittsburgh)
- ✅ Broken Steel — extends the Capital Wasteland (no new worldspace of note); loaded with the game's
  plugins in its own order, so its changes are in the Wasteland we read
- ✅ Point Lookout — Maryland (with the Sacred Bog; its lighthouse at the real one)
- ➖ Mothership Zeta — in orbit

## Fallout: New Vegas (2010) — tier 1 (Mutagen)
- ✅ Mojave Wasteland (`WastelandNV`), incl. its LOD ring; Freeside/Strip/Westside child
  worldspaces contribute their map markers only
- ✅ Dead Money — the Sierra Madre (seven exteriors; at Grand Canyon West)
- ✅ Honest Hearts — Zion Canyon (anchored to Zion National Park)
- ✅ Old World Blues — Big MT (at Shoshone, CA, south of the Divide); Higgs Village left out
  (a warehouse interior)
- ✅ Lonesome Road — the Divide (five exteriors; at Death Valley Junction); the nuked Long 15
  and Dry Wells come through the Mojave's child links
- ➖ Gun Runners' Arsenal, Courier's Stash packs — items only

## Fallout 4 (2015) — tier 1 (Mutagen, `Mutagen.Bethesda.Fallout4`; official plugins and the
Creation Club plugins listed in Fallout4.ccc)
- ✅ The Commonwealth (Boston), incl. its modelled land around; Diamond City and Goodneighbor
  contribute their map markers; the Glowing Sea's ground reads as Radiation
- ✅ Far Harbor — the Island, at Mount Desert Island, Maine (six anchors: the town of Far Harbor is
  Bar Harbor; Acadia, Echo Lake, Southwest Harbor, Cranberry Island, the park visitor center); not
  the DiMA memory simulation
- ✅ Nuka-World — no canon location; at Six Flags New England (Agawam, MA), south of Springfield
  on the Connecticut River, per fan research matching its map
- ➖ Automatron, Wasteland Workshop, Contraptions Workshop, Vault-Tec Workshop — no new land
  (Vault 88 is an interior)
- ➖ Creation Club / Creations — none of the installed ones adds a worldspace; their quests are set in
  the Commonwealth (the Enclave Remnants' camps, Sea Scavengers in the Glowing Sea) and load with it

## Fallout Shelter (2015)
- ➖ No geography

## Fallout 76 (2018) — tier 2 (fo76utils, built locally by `build.rs`; Mutagen does not read it)
- ✅ Appalachia (West Virginia), incl. its LOD ring (the terrain file runs ~30–50 cells beyond the
  game's map); later updates' content is in the same worldspace
- ✅ Skyline Valley (Shenandoah, Virginia; 2024) — part of Appalachia, along its south edge; not
  anchored (the game moves it ~100 mi from the real Shenandoah)
- ✅ Burning Springs (southern Ohio; 2025) — part of Appalachia, west of the Ohio River
- ✅ The Pitt (Pittsburgh; 2022) — an expedition: interior zones dressed as outdoors (Industrial, Trench,
  Foundry, Sanctum, Steel Tower), each at its real counterpart, terrain only; Fallout 3's Pitt carries
  the marker and the two vote together (`EXM1PittWorldspace` is an empty placeholder)
- ✅ Atlantic City (New Jersey; 2024) — an expedition: seven interior zones (Boardwalk, Showman's Pier,
  Aquarium, Casino Quarter, City Hall, Community Center, the flooded downtown), one "Atlantic City" marker
- ⏸ Raven Rock (2027) — announced, not yet in the game files

## Fallout (TV series)
- ✅ Season 1 locations (manual JSON in `input/manual/tv`)
- ⬜ Season 2 (New Vegas) locations, as manual JSON

## Other media
- ⬜ Fallout: The Roleplaying Game setting guides (Modiphius): New Vegas Setting Guide (2026), a New
  California guide (Fallout 1's era), Winter of Atom (the Commonwealth); regional maps to check
- ➖ Fallout: The Board Game (FFG, with New California and Atomic Bonds), Fallout: Factions and
  Wasteland Warfare's Commonwealth and Nuka-World expansions: places the games already map
- ➖ Comics (One Man and a Box, All Roads), MTG cards, art books, Fallout Shelter: nothing new to place
- ⏸ TV series season 3 (filming from mid-2026; setting not announced)

## Tabletop
- ✅ Fallout: Wasteland Warfare — NCR map (manual JSON in `input/manual/tabletop`)
- ➖ Fallout: The Roleplaying Game — checked, little usable map data (a long-shot source)
- ⬜ Exodus (2008, Glutton Creeper Games) — began as Interplay's licensed Fallout d20 RPG; Bethesda had
  the licence annulled in 2007 and it shipped without the Fallout names. The American Southwest in 2042
  (New Mexico, Arizona, California, northern Mexico, Utah, Nevada); tier 5

## Mods — tier 6
Fetched by `src/mods.rs` from `input/config/mods.json` (ModDB, Mediafire; hash-checked). Their
markers only anchor the warp; their terrain adds detail. Nexus files are downloaded by hand into
`~/Downloads` (a run names the missing files and their pages), then copied and checked like the rest.
GamePressure mirrors of Nexus files are acceptable where they exist.

### Fallout 2 engine (`interplay.rs`; cluster maps `mod_fallout2_<id>`)
- ✅ Fallout: Nevada — Nevada and Utah before Fallout 1 (Reno to Salt Lake City and Las Vegas)
- ✅ Fallout: Sonora — Arizona, Sonora and Baja California (Phoenix to Hermosillo)
- ✅ Fallout 1.5: Resurrection — New Mexico between Fallout 1 and 2 (ModDB's EN 1.5 installer, over Fallout
  2's master.dat); anchored by Albuquerque and Rat Hole (Kirtland, NM, as its dialogue says)
- ➖ Mutants Rising — Elko, Nevada; only a 2011 demo is out (ModDB). Its world map is Fallout 2's (814 of
  840 squares the same terrain, the Pacific coast included) with the towns renamed for Nevada and Utah:
  no terrain of its own. Revisit if a full release redraws the map
- ➖ MIB88 Megamod — Fallout 2's own region with towns restored and added (Carson City, Vaults 14 and 23...):
  skipped
- ➖ Fallout Yesterday — Van Buren remade in the Fallout 2 engine, the same area as the Van Buren map we
  read (all its places within ~30 mi of that map): skipped
- ➖ Olympus 2207 — not set in the Fallout universe
- ➖ Last Hope (2009; v1.088, NMA) — its own mountain world map with fictional towns (Philton, Canville,
  Miners Town, Golden Valley...): nothing real to anchor it
- ➖ Fallout: Between Good & Evil (Czech) — eastern/southern Arizona and northern Mexico before Fallout 1;
  never released (its site, towerofcreation.com, now archived: no release by 2012, none since)
- ➖ Vault Rats (2007) — a short parody on Fallout 2's own world map (no world map or towns of its own)
- ➖ FOnline: 2238 / Reloaded (Fallout 1 and 2's worlds merged: land we already map); FOnline: Ashes of
  Phoenix (one city, no regional map)
- ➖ Fallout: Bakersfield (Doom engine remake of Fallout 1's Necropolis, due 2027)

### Fallout Tactics engine (campaign world maps, `bos.cam`, as `tactics.rs` reads)
- ✅ Fallout Tactics: Crusade (2020; v1.2.3, ModDB) — the Texas Gulf Coast around Victoria (its own map,
  ~27x19 mi), anchored by Victoria, Bloomington, Port Lavaca and Vanderbilt (Garcitas Creek, a creek, fits
  12 mi off; La Salle and Guada are not placeable)
- ➖ Fallout: The Wastes (2009, ModDB) — California, Klamath to the Channel Islands; only land Fallout 1 and 2
  already map, and its places do not register with ours (its NCR sits twice as far from Vault 13 as the
  real ones: sheared or, without it, tens of miles off the coast)
- ➖ Fallout: Awaken (2003; 1.41d English patch, ModDB) — fictional towns ("New Chicago", "far to the
  west" of Los Angeles) with Fallout 1's names in another layout: nothing real to anchor it
- ➖ Fallout: Enclave (2019, ModDB) — its own map (a salt lake in the south-west, snowy ranges east) with
  Van Buren's names garbled in translation ("New Cannon", "Boomfield", "Max Bunker") in an order Van
  Buren's real places contradict: nothing reliable to anchor it
- ➖ Lost in the Wastelands (2011; v1.2, Russian; ModDB via RobCo Mod Launcher) — its own small map with
  fictional places (a dump that wants to join the NCR, Vault 35, caches, a cave): nothing real to anchor it
- ➖ Darkfall (2008 demo) — Fallout Tactics' own campaign map (identical art) with fictional towns
- ➖ Fallout Tactics Redux, xkcon's Ultimate Rip Off Mod — the original campaign reworked (its own map)
- ➖ Project Flagstaff (2020 demo) — Arizona; a test map only, no campaign world map
- ⏸ Fallout Tactics 2: Long Live The King (south-east US and the Caribbean), Fallout: Enclave II,
  Project Safehouse, Fallout Resurrection (Tactics) — not released

### Bethesda games
Plugins are loaded after the game's masters (Mutagen); a mod's own world keeps only where it is built,
and a mod extending a game's worldspace keeps only the cells it adds or reshapes. Anchors are written
per source (`REAL_COORDS` is grouped by cluster).
- ➖ Out of scope: Fallout: London, Sakhalin (outside the US)

### New Vegas: new land at real places (terrain + markers, anchorable)
- ✅ Fallout: New California — the Pass: the San Bernardino Mountains from the Cajon Pass to Big Bear
  Lake, anchored by 7 real places (I-15 from Hesperia through the pass, Barstow, the Mormon Rocks, SR-138
  to Crestline, Big Bear Lake), only the land within 3 cells of them (its world stretches ~135 mi); radiation
  only from its markers of 10 rads or more (not its ambient zones). Read from ModDB's BETA 222 patch (86 MB), whose NewCalifornia.esm is the
  whole mod's plugin, not the 7.9 GB installer (BETA 231; also Nexus 45138, GamePressure, GOG)
- ✅ Fallout Atlanta (alpha 0.65, ModDB; also Nexus 61872) — Atlanta, GA, anchored by 12 real places
  (Stone Mountain, Westview Cemetery, Vine City, Grant Park, East Lake, Riverdale...)
- ➖ Fallout: Free Cheyenne (demo 1.0.2, ModDB) — Cheyenne, WY: dropped. Its town spreads downtown
  places that lie within 0.6 mi of each other over ~4 mi: anchored, it shrinks to one hex; placed by the
  town alone, it is larger than the real city
- ➖ Sun Valley (v0.45, ModDB) — "Hoover City" and Beavercreek, through a tunnel from the Mojave's
  north-west: no real place to anchor it
- ✅ Mojave National Preserve (Ghoulville; Nexus 57373) — the land south of Nipton it adds to the Mojave,
  placed by the Mojave's warp and Kelso and the Dead Mountains
- ✅ Beyond the Borders – New Lands (Nexus 39427) — the land past the Mojave's borders (cells it adds or
  reshapes), placed by the Mojave's warp
- ✅ Arizona Expedition (Nexus 60866) — the Arizona bank of the Colorado, placed by the Mojave's warp and
  Willow Beach and Bullhead City
- ✅ Beyond Boulder Dome (Nexus 46907; its ESM file) — Boulder, Colorado: the city, dome and the wastes
  around it (child worlds of Boulder Mountain Park), placed by Boulder (not Outer Nevada, nor the
  battle copy of Inner Boulder)
- ✅ Dog City Denver (Nexus 41751) — Dog City (Van Buren's Denver), placed by Denver
- ✅ BFM Denver (Nexus 58761; its ESM) — Denver, placed by the city (not its Elitch Gardens world, ~10 mi
  around a park of a few blocks, over Denver itself, nor its stadium); needs Speedy Resources (39551)
- ✅ BFM San Francisco (Nexus 60152) — San Francisco's outskirts and bay, placed by the city; needs Speedy
  Resources
- ➖ Project Pacific (Nexus 50789) — Anacapa Island, one of California's Channel Islands: dropped; at the
  game's scale its island is ~270 sq mi (the real one ~1), over Santa Cruz Island too
- ✅ Nova Arizona (Nexus 85915) — the Arizona side of the Colorado it adds to the Mojave, placed by the
  Mojave's warp and 10 real places (Willow Beach, Arizona Hot Spring, Temple Bar, White Hills, Dolan
  Springs, Chloride, Grasshopper Junction...)
- ✅ Zion Trail (Nexus 61939) — the walk from the Desert National Wildlife Refuge to Zion, anchored by
  Corn Creek on US 95, Mormon Well, Hayford Peak and Zion's entrance and lodge
- ✅ Havasu Blues (Nexus 76680) — Lake Havasu City, anchored by Pittsburg Point and the airport (Catfish
  Bay and Site Six are real but not found to place)
- ✅ San Fran (pre-alpha; Nexus 70128) — San Francisco's north-east waterfront and the bay's islands,
  anchored by 14 real places (Ferry Building, Embarcadero, Telegraph Hill, Coit Tower, Pier 39, North
  Beach, Yerba Buena and Treasure Islands...); its Chinatown ("Shi-town") placed by its name
- ✅ Fallout Chicago (demo; Nexus 96929, its ESM only) — the Loop and Near North Side, anchored by Willis
  Tower, Chinatown, Cabrini-Green, the Second City, Stanton Park, Marina City, Portillo's; built near street
  scale (~5x4 real mi). Not Millennium Park (~1.9 mi east of Willis Tower at the others' scale, 0.7 in reality)
- ✅ Russell (Nexus 50107) — Caruthers Canyon in the New York Mountains (Mojave National Preserve), placed
  by its name; needs the Someguy Series master (48925)
- ✅ Long 15 – NCR Expansion (Nexus 94306) — the Long 15 before Lonesome Road's nuke: Baker, California
  and the land east of it, anchored by Baker, the Hollow Hills and Kelso
- ➖ New Vegas Bounties III (Nexus 55744) — the Deep Creek Mountains, Utah (its dialogue: "up to Utah"):
  dropped; its world is a snowy valley ringed by mountains (~24 mi) with fictional places, nothing to
  match the real north-south range by
- ✅ NWCE – Necropolis Addon (Nexus 54537) — its Carson City, Nevada, placed by its name (~8x5 mi, as the
  real city); not its Necropolis (~20x35 mi at the game's scale for a ruined town, under a mile at its true
  size) nor its other districts (separate worlds, no stated layout)
- ✅ The Empire Wasteland – Project Big Apple (alpha; Nexus 53343) — New York, anchored by the George
  Washington Bridge, Washington Heights (168th St), Times Square and Jersey City (not its Hoboken, "Mile
  Square City": ~170 sq mi at the game's scale for a city of one)
- ✅ Fallout: The Frontier (Nexus 68009; its plugin alone from archive.org's upload of the original
  release, 249 MB of a 13 GB archive) — Portland, Oregon, anchored by 14 real places (Burnside Bridge,
  Skidmore Fountain, Pearl District, Old Town, Nob Hill, Goose Hollow, Council Crest, Irvington, the
  Convention Center, the Marquam Bridge...), only the land around them (its world is 128 cells square)
- ➖ Not doing: Los Angeles (two markers in a 1,775-cell world), Caesar's 5th Cohort ("Reno Valley Basin":
  1,849 cells, little built), New Vegas Railroads (Bullhead) and Baker in the Sun (small worlds), North
  Nevada Open (Russian-only markers), Into the Deep (6 cells by Ivanpah), AEW (needs Invisible Wall Remover), Tales
  from the Burning Sands (its own world is a fictional campus), Path to Canaan (a fictional Utah valley),
  Project Mojave (the New Reno Pass, few real places; needs Mojave Master Plugin), Skidge's Quest
  Pack (24 cells of the Mojave), Dry Wells – Legion Expansion (Dry Wells is unplaced: "along Interstate
  40" at most; its places are fictional), Salt Lake Stories (its worlds' places are fictional; its
  Bonneville world is a bare heightmap, its Stansbury Bay ~20x20 mi for a ~11x4 mi island; its travel map
  has no markers), Tales of New Reno Episodes Zero to Two (Zero's "No Man's Land" has no markers
  or stated place; One's own worlds are a fictional listening post and a dream, its Mojave edits fictional;
  Two's "Canyonlands" scatters real Utah names freely: Springdale, Zion's tunnel, Big Spring Canyon, Thompson
  and Wellington in an order reality contradicts)
- ➖ Rig City Areas — needs several other mods' masters (MoMod, Mothership Zeta Content Restored...)
- ➖ Not released: Up Country (Michigan), Enclave City: New Cincinnati, Broken City, Oregon's Grand
  Vault; Fallout: Nuevo Mexico (cancelled, January 2025)

### New Vegas: other mods
- ➖ Fallout: The New West (formerly Revelation Blues) — Black Isle's cancelled Fallout 3 (Van Buren)
  recreated in New Vegas (demos released, Nexus): the Van Buren map already covers its land
- ⬜ More of the Mojave (being checked for land they add): A World of Pain, MoreMojave, Westside Suburbs
  Uncut, Primm South Uncut, Mojave Outpost Car Apocalypse Uncut, Southern Mojave Expanded (Nipton, Nelson,
  Quarry Junction), New Vegas Uncut (A Wilder Wasteland, Outside Bets, Freeside Open), Simple Open Strip
- ➖ Median Valley (Nexus 61568; setting not found), Fallout: Forgotten Valley (a fictional valley)

### New Vegas: nothing to place
- Autumn Leaves (a library by Novac), Garden of Ethan (a pre-war facility by Nellis): enclosed
  worldspaces, interiors in effect
- The Inheritance, The Better Angels, New Vegas Bounties I–II, Alien Exterminator, Cabot's Quest,
  The World We Knew: quests in the Mojave and interiors (Better Angels and The Inheritance to check)
- Tale of Two Wastelands — Fallout 3's lands in New Vegas's engine; we read Fallout 3 directly

### Fallout 3: new land at real places (worldspaces; Nexus files downloaded by hand)
- ✅ Alton IL — Alton, Illinois, placed by the town at Fallout 3's scale (its places are laid out
  freely); with New Godfrey (at Godfrey, IL) and its Dulles International Airport (Dulles, VA). Not
  Belleview nor the BoS Nature Preserve (fictional); Nexus 16949
- ✅ Into the Ark — Newark, New Jersey (and the Ironbound), anchored by its real sights; its Philadelphia
  has no landscape; Nexus 21780
- ➖ Wrong Turn — fictional hill country in West Virginia, no place to anchor it: skipped; Nexus 17526
- ➖ Anchorage Aftermath (Nexus 22034), Back to Anchorage (10720), Anchorage - 2277 (21677, unfinished) —
  Anchorage after the war: the land Operation Anchorage's simulation already maps
- ➖ The Scourge 2255 — the Pitt in 2255 (its worldspaces copied and expanded); Nexus 27029
- ✅ Fallout 3 Anniversary - District of Columbia — the 13 cut DC worldspaces restored and placed where they
  were meant to be; Nexus 27168
- ✅ Beyond the Wasteland's borders (cells they add to or reshape in the Wasteland): Vault 89 -
  Burkittsville (Burkittsville, MD; Nexus 18560), Northwest Forest(-Redone) (past Raven Rock; 17758)
- ✅ Project Eastern Wasteland (alpha) — Riverdale, Maryland: the town and its wasteland, at Riverdale
  Park (not its Eastern Ruins, a 64-cell square of default land built in a few rows); Nexus 16183
- ➖ Inside the Capital Wasteland (land we map): Fallout World Expansion, DCXpansion Stanton Park, Stories
  from Point Lookout, The Secrets of Harrisport, Red Line North (metro cells), VT Island
- ➖ Fictional places: Valley Wasteland, Heads of Iron, Island - Northern Territory, The Assassin, Alpha -
  The New World, Death Canyon, Fallout Zero, Alien Resurrection, The Librarian, Green Killzone, A Town Far
  Far Away, Evil Forces
- ➖ Outside the US: Reykjavik (Iceland), Saint Petersburg, Karlskrona (Sweden), Nostalgia (Chernobyl);
  Operation: Kingpin (New Owen Sound, Ontario, ~110 mi from Michigan; never released)
- ➖ Unreleased: Fallout remake (Fallout 1 in Fallout 3), The Deep South

### Fallout 4 (worldspaces; Nexus files downloaded by hand)
- ✅ Project Empire (Nexus 12890) — Puget Sound, Washington (its world is called "Northwest Commonwealth"):
  its heightmap of the sound kept whole around 31 real towns (Seattle, Tacoma, Olympia, Everett, Bremerton,
  Port Angeles, Victoria...)
- ✅ Massachusetts Project (beta; Nexus 64655) — Cape Ann, the land it adds past the Commonwealth's
  north-east border: the Commonwealth's warp and 8 real places (Rockport, Gloucester, Eastern Point,
  Annisquam, Manchester-by-the-Sea, Agassiz Rock, Crane Estate, Newbury), one per locale
- ✅ Fallout Vermont (Nexus 86755) — central Vermont, Montpelier to Tunbridge (9 anchors)
- ✅ Fallout New Hampshire (Nexus 87381) — southern New Hampshire, Nashua to Manchester (7 anchors)
- ✅ Connecticut (Nexus 93927) — north-central Connecticut, Cromwell to Andover (6 anchors)
- ✅ The Borders of the World (Nexus 72600), Beyond the Borders – New Lands (Nexus 17255) — land past the
  Commonwealth's borders, placed by its warp (their places are fictional)
- ✅ Fallout Miami (early version; Nexus 24737, its 1.03 update: the plugin is the whole mod's) — Miami, placed by the
  city (its places are laid out freely: its Liberty District east of Lincoln Road, the real Liberty City
  west of Miami Beach)
- ⬜ Being checked: Miami Misadventures (Nexus 58073; Miami, episodic), Endless Hypetrain (21903; Chicago)
- ➖ Operation Manhattan (24216; New York, which The Empire Wasteland covers), DLC76 (36118; Charleston and
  Vault 76, land Fallout 76 maps)
- ➖ Northern Springs (Nexus 32911) — fictional throughout (Wolfendale, Sanatogan, Hillsgrove...), reached
  from the Commonwealth's north-west corner
- ➖ Isles of New England: Boon Island (Nexus 84449, ~11 mi at the game's scale for a ~0.1 mi rock),
  Misery Island (87983, ~24 mi for a ~0.5 mi island); Thompson Island (13469, its harbour islands freely
  arranged: Moon Island north of Long Island)
- ➖ Fallout 4 – Point Lookout, Project Mojave — ports of land Fallout 3 and New Vegas map; Fallout: Vault 13
  (demo, ModDB) — Fallout 1's land, a 5 GB archive
- ➖ Fictional: Fusion City Rising, Outcasts and Remnants, Penn's Woods and many islands; outside the US:
  Fallout: London, Sakhalin, Fallout – Costa Rica
- ⏸ Not released: Fallout: Cascadia (Seattle), Fallout Miami (full), Fallout 4: New Vegas, Fallout 4: The
  Capital Wasteland, Kanawha, Niagara, Wyoming, Billings, North Star (Minnesota), New York, Chicago,
  Ouachita, Red Menace

### Other engines (world maps we could read)
- ➖ Fallout Tactics engine: Fallout: The Sum — not set in the Fallout universe

## Next
From the remaining games, Fallout 76 is nearest to what is mapped: Burning Springs (southern Ohio)
is ~110 mi and Appalachia ~160–190 mi from the Tactics map's eastern edge (Indiana), against
~430 mi for Fallout 3's Washington, DC and ~700 mi for Fallout 4's Boston.
