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
- ✅ Southern California world map (`interplay.rs`; with Fallout 2 in the `interplay` cluster)

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
- ⬜ Broken Steel — extends the Capital Wasteland (no new worldspace of note)
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
markers only anchor the warp; their terrain adds detail. Nexus files need Premium or a manual
download (see TODO.md). GamePressure mirrors of Nexus files are acceptable where they exist.

### Fallout 2 engine (`interplay.rs`; cluster maps `mod_<id>`)
- ✅ Fallout: Nevada — Nevada and Utah before Fallout 1 (Reno to Salt Lake City and Las Vegas)
- ✅ Fallout: Sonora — Arizona, Sonora and Baja California (Phoenix to Hermosillo)
- ✅ Fallout 1.5: Resurrection — New Mexico between Fallout 1 and 2 (ModDB's EN 1.5 installer, over Fallout
  2's master.dat); anchored by Albuquerque and Rat Hole (Kirtland, NM, as its dialogue says)
- ⏸ Mutants Rising — Elko, Nevada; only a 2011 demo is out (ModDB): wait for a full release (Nexus or
  wherever it appears)
- ➖ MIB88 Megamod — Fallout 2's own region with towns restored and added (Carson City, Vaults 14 and 23...):
  skipped
- ➖ Fallout Yesterday — Van Buren remade in the Fallout 2 engine, the same area as the Van Buren map we
  read (all its places within ~30 mi of that map): skipped
- ➖ Olympus 2207 — not set in the Fallout universe

### Bethesda games
- ⬜ Next: Fallout: New California (terrain; ModDB's full installer BETA 231, or GOG's build)
- ➖ Out of scope: Fallout: London, Sakhalin (outside the US)

### New Vegas: new land at real places (terrain + markers, anchorable)
- Fallout: New California — Black Bear Mountain National Forest, Vault 18 in the Cajon Pass
  (San Bernardino Mountains, CA); a worldspace ~65% the Mojave's size; Nexus 45138, ModDB
  (`falloutprojectbrazil`: full installer BETA 231, 2018), free on GOG with a one-click install
- Fallout: The Frontier — Portland, Oregon (snowbound); larger than the Mojave; Nexus only (its ModDB
  page has no files)
- Long 15 – NCR Expansion — the Long 15 before Lonesome Road's nuke, and NCR lands in California;
  Nexus 94306
- Dry Wells – Legion Expansion — Dry Wells before the nuke, and Legion lands in Arizona; Nexus 88180
- Nova Arizona – A Legion Lands Expansion — the land east of the Colorado River; Nexus 85915
- Salt Lake Stories — around the Great Salt Lake (15 small worldspaces); Nexus 63332
- Tales of New Reno – Episode Zero — a New Reno worldspace (Reno, NV; Fallout 2's New Reno);
  GamePressure mirror
- Fallout: Atlanta — Atlanta, GA (alpha, in development since 2016; v0.6.0 on GamePressure, 2026)
- Sun Valley — NCR, Legion and Khans in "Sun Valley" with Hoover City, by tunnel from New Vegas
  (v0.45 beta, 2011; ModDB); real place to check
- ➖ Fallout: Nuevo Mexico — New Mexico; cancelled (January 2025)

### New Vegas: new land with no stated real place (terrain; placement needs canon/fan theory)
- Median Valley — a quest mod in its own worldspace; Nexus 61568 (setting not found)
- Fallout: Forgotten Valley — a fictional valley (2011, Russian; English translation 2026)

### New Vegas: more of the Mojave (markers, some terrain, in worldspaces we already map)
- Beyond the Borders – New Lands — opens the Mojave's borders: ~20 dungeons, a town, an NCR base
  in the land outside them (edits WastelandNV itself)
- A World of Pain — ~170 new locations across the Mojave
- Uncut Wasteland / New Vegas Uncut — restored cut locations and NPCs; Freeside Open reworks the
  Freeside worldspace
- Simple Open Strip — merges the Strip into the Mojave (removes the gates)

### New Vegas: quest mods the guides recommend that bring their own land
- New Vegas Bounties III (Someguy series) — a frigid new worldspace in Utah
- Russell (Someguy series) — companion mod with a "Mesquite Valley" worldspace (the Mesquite
  Mountains west of Goodsprings, or Mesquite, NV? to check)
- Beyond Boulder Dome — three worldspaces at Boulder, Colorado (a Van Buren location); GamePressure
  mirror
- Fallout: The New West (formerly Revelation Blues) — recreates Black Isle's cancelled Fallout 3
  (Van Buren) in New Vegas (demos released, Nexus)
- Havasu Blues — a ghoul settlement in Arizona (Lake Havasu? to check)
- Into the Deep — a new town, Shamble, by Ivanpah Dry Lake near Nipton
- Skidge Quest Pack — twelve quests and a new town, Sue Town (location to check)
- Tales from the Burning Sands — serialized chapters, each with a new location (land to check)

### New Vegas: nothing to place
- Autumn Leaves (a library by Novac), Garden of Ethan (a pre-war facility by Nellis): enclosed
  worldspaces, interiors in effect
- The Inheritance, The Better Angels, New Vegas Bounties I–II, Alien Exterminator, Cabot's Quest,
  The World We Knew: quests in the Mojave and interiors (Better Angels and The Inheritance to check)
- Tale of Two Wastelands — Fallout 3's lands in New Vegas's engine; we read Fallout 3 directly

### Other engines (world maps we could read)
- ➖ Fallout Tactics engine: Fallout: The Sum — not set in the Fallout universe
- Fallout 3, new land at real places: Alton, IL (Alton, Illinois; twice Point Lookout's size; Nexus)
- Fallout 4, new land at real places: Northern Springs (northern Massachusetts; Nexus); not released:
  Fallout: Cascadia (Seattle), Fallout Miami, Fallout: Vault 13 (Fallout 1 remade), Fallout 4: New
  Vegas, Fallout 4: The Capital Wasteland, Kanawha (West Virginia), Niagara, Wyoming, Billings
- ➖ Fallout 4: Fallout: London, Sakhalin — outside the US, out of scope

## Next
From the remaining games, Fallout 76 is nearest to what is mapped: Burning Springs (southern Ohio)
is ~110 mi and Appalachia ~160–190 mi from the Tactics map's eastern edge (Indiana), against
~430 mi for Fallout 3's Washington, DC and ~700 mi for Fallout 4's Boston.
