# TODO

## Later
- Extract positional data from the games' map visuals (world map art: coastlines, rivers, roads, mountain ranges), not only from CITY.TXT/WORLDMAP.TXT. Such features could serve as extra control points or constraints for the un-rubbersheeting, alongside the marker anchors.

## Bethesda games (Mutagen)
- Fallout 3 and Fallout 4 on the Mutagen path (FO4 via `Mutagen.Bethesda.Fallout4`).
- Mod plugins: done for Fallout 3 (`mods.rs` manifest; Nexus files downloaded by hand into
  `~/Downloads`, as portmod does, since Nexus API download links need Premium). Next: New Vegas, then
  Fallout 4 (its helper does not take mod plugins yet). Optionally: Nexus downloads through the API
  when a Premium key is set.
- Mutagen 0.54.4 cannot read compressed FO3/NV BSA entries (`BsaFileRecord.AsStream` wraps an `InflaterInputStream` in a `FramedStream` that asks for its `Length`); report upstream if BSA contents are ever needed.

## Warp
- `RubberSheet::new_fold_free` resolves folds greedily (smallest single move each time): a ~1 mi anchor change can cascade into larger corrections elsewhere (Tactics: geocoding Springfield/Great Bend moved Buena Vista 18.5 mi). That may be genuine (the games rubbersheet unevenly); if results look off, try minimising total displacement instead.
