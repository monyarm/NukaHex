# TODO

## Later
- Extract positional data from the games' map visuals (world map art: coastlines, rivers, roads, mountain ranges), not only from CITY.TXT/WORLDMAP.TXT. Such features could serve as extra control points or constraints for the un-rubbersheeting, alongside the marker anchors.

## Bethesda games (Mutagen)
- New Vegas DLC worldspaces (Sierra Madre, Big MT, the Divide), placed at real locations through canon or fan theory.
- Fallout 3 and Fallout 4 on the Mutagen path (FO4 via `Mutagen.Bethesda.Fallout4`).
- Download mod plugin files (tier 5), after every Bethesda game is supported.
- Mutagen 0.54.4 cannot read compressed FO3/NV BSA entries (`BsaFileRecord.AsStream` wraps an `InflaterInputStream` in a `FramedStream` that asks for its `Length`); report upstream if BSA contents are ever needed.

## Warp
- `RubberSheet::new_fold_free` resolves folds greedily (smallest single move each time): a ~1 mi anchor change can cascade into larger corrections elsewhere (Tactics: geocoding Springfield/Great Bend moved Buena Vista 18.5 mi). That may be genuine (the games rubbersheet unevenly); if results look off, try minimising total displacement instead.
