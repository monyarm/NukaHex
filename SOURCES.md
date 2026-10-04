# Sources checklist

Every Fallout game and add-on, and whether the map uses it. Tiers (vote weight and build order;
`schema::tier`): 1 main games and manual locations, 2 Fallout 76, 3 simulations of the past
(Operation Anchorage), 4 Tactics and Van Buren, 5 Brotherhood of Steel, 6 mods. Vote weights
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
- ✅ Colorado / Utah / Arizona world map (`van_buren.rs`)

## Fallout: Brotherhood of Steel (2004, PS2/Xbox) — tier 5
- ⏸ Carbon, Los (Dallas), the Vault — deferred: needs the engine's mesh/material format
  (`.claude/plans/fallout_bos.md`)
- ➖ Fallout: Brotherhood of Steel 2 — cancelled

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

## Tabletop
- ✅ Fallout: Wasteland Warfare — NCR map (manual JSON in `input/manual/tabletop`)
- ➖ Fallout: The Roleplaying Game — checked, little usable map data (a long-shot source)

## Mods — tier 6
Fetched by `src/mods.rs` from `input/config/mods.json` (ModDB, Mediafire; hash-checked). Their
markers only anchor the warp; their terrain adds detail. Nexus files need Premium or a manual
download (see TODO.md).

### Fallout 2 engine (`interplay.rs`; cluster maps `mod_<id>`)
- ✅ Fallout: Nevada — Nevada and Utah before Fallout 1 (Reno to Salt Lake City and Las Vegas)
- ✅ Fallout: Sonora — Arizona, Sonora and Baja California (Phoenix to Hermosillo)
- ⏸ Fallout 1.5: Resurrection, Mutants Rising, MIB88 Megamod, Fallout Yesterday — to check
- ➖ Olympus 2207 — not set in the Fallout universe

### Bethesda games
⏸ all deferred.

### New Vegas: new land at real places (terrain + markers, anchorable)
- Fallout: New California — Black Bear Mountain National Forest, Vault 18 in the Cajon Pass
  (San Bernardino Mountains, CA); a worldspace ~65% the Mojave's size; Nexus 45138, also free on
  GOG with a one-click install
- Fallout: The Frontier — Portland, Oregon (snowbound); larger than the Mojave
- Long 15 – NCR Expansion — the Long 15 before Lonesome Road's nuke, and NCR lands in California;
  Nexus 94306
- Dry Wells – Legion Expansion — Dry Wells before the nuke, and Legion lands in Arizona; Nexus 88180
- Nova Arizona – A Legion Lands Expansion — the land east of the Colorado River; Nexus 85915
- Salt Lake Stories — around the Great Salt Lake (15 small worldspaces); Nexus 63332
- Tales of New Reno – Episode Zero — a New Reno worldspace (Reno, NV; Fallout 2's New Reno)
- Fallout: Atlanta — Atlanta, GA (alpha, in development since 2016)
- Fallout: Nuevo Mexico — Mexico City (in development, no release)

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
- Beyond Boulder Dome — three worldspaces at Boulder, Colorado (a Van Buren location)
- Fallout: Revelation Blues — recreates Black Isle's cancelled Fallout 3 (Van Buren) in New Vegas
  (demo released)
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
- Fallout 4: Fallout London, Fallout: Cascadia, Fallout Miami, Fallout: Vault 13 (Fallout 1 remade),
  Fallout 4: New Vegas, Fallout 4: The Capital Wasteland and others (after Fallout 4 itself)

## Next
From the remaining games, Fallout 76 is nearest to what is mapped: Burning Springs (southern Ohio)
is ~110 mi and Appalachia ~160–190 mi from the Tactics map's eastern edge (Indiana), against
~430 mi for Fallout 3's Washington, DC and ~700 mi for Fallout 4's Boston.
