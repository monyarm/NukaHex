# TODO

## Later
- Extract positional data from the games' map visuals (world map art: coastlines, rivers, roads, mountain ranges), not only from CITY.TXT/WORLDMAP.TXT. Such features could serve as extra control points or constraints for the un-rubbersheeting, alongside the marker anchors.

## Bethesda games (Mutagen)
- Fallout 3 and Fallout 4 on the Mutagen path (FO4 via `Mutagen.Bethesda.Fallout4`).
- Download mod plugin files (tier 5), after every Bethesda game is supported. Findings (2026-10-03):
  - We only need the files (plugins + BSAs) to hand to the Mutagen helper with an explicit load
    order, not a mod manager (no install, conflicts or VFS).
  - portmod (CLI, Python, last release 2.9.1, 2025-05) has Fallout NV (56 packages, mostly
    gameplay/visual) and Fallout 4 (stub; one landmass, a Point Lookout port) repositories, but
    no worldspace mods we would map. Its Nexus support does not download: it prints the links
    and waits for manual downloads; only direct URLs are fetched.
  - Nexus API download links require Premium; free accounts go through nxm:// (a manual click).
  - Plan: a small manifest in the repo (mod, game, file URL or Nexus mod/file id, sha256,
    plugins), fetching direct URLs (ModDB/GitHub/own sites) automatically, Nexus files via the
    API when a Premium key is set, else portmod-style "download these into input/mods".
- Mutagen 0.54.4 cannot read compressed FO3/NV BSA entries (`BsaFileRecord.AsStream` wraps an `InflaterInputStream` in a `FramedStream` that asks for its `Length`); report upstream if BSA contents are ever needed.

## Warp
- `RubberSheet::new_fold_free` resolves folds greedily (smallest single move each time): a ~1 mi anchor change can cascade into larger corrections elsewhere (Tactics: geocoding Springfield/Great Bend moved Buena Vista 18.5 mi). That may be genuine (the games rubbersheet unevenly); if results look off, try minimising total displacement instead.
