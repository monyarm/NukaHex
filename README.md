# Fallout Map Generator

A Rust-based tool for extracting, processing, and georeferencing maps from various Fallout games (Fallout 1, 2, 3, New Vegas, 4, and 76) into Tiled map format.

## Overview

This project generates Tiled maps (`.tmx`) and tilesets (`.tsx`) from Fallout game data, with the ultimate goal of creating a unified, georeferenced map of the United States by combining data from all Fallout games. The tool corrects distorted game map scales and positions using real-world anchor points.

## Supported Sources

| Source | Status | Extraction Method | Notes |
|--------|--------|-------------------|-------|
| Fallout 1 | ✅ Supported | Automatic (Steam/GOG) | Full extraction |
| Fallout 2 | ✅ Supported | Automatic (Steam/GOG) | Full extraction |
| Fallout 3 | ✅ Supported | Automatic (Steam), via Mutagen | Capital Wasteland, The Pitt, Point Lookout and Operation Anchorage (an Alaska inset), all DLC loaded |
| Fallout: New Vegas | ✅ Supported | Automatic (Steam), via Mutagen | Mojave and all four add-on areas (Sierra Madre, Zion, Big MT, the Divide) |
| Fallout 4 | ✅ Supported | Automatic (Steam), via Mutagen | The Commonwealth, Far Harbor and Nuka-World, all DLC and Creation Club plugins loaded |
| Fallout 76 | 🔧 Planned | - | No Mutagen support; needs another reader |
| Fallout Tactics | ✅ Supported | Automatic (Steam) | World map and towns |
| Van Buren (tech demo) | ✅ Supported | Automatic | World map art and markers |
| Fallout: Nevada, Fallout: Sonora (mods) | ✅ Supported | Automatic download (`input/config/mods.json`) | Terrain only (tier 6); their towns anchor the warp |
| Fallout TV Series (2024) | ✅ Supported | Manual JSON definitions | Locations manually defined |
| Fallout: Wasteland Warfare (Board Game) | ✅ Supported | Manual JSON definitions | Locations manually defined |

See [SOURCES.md](SOURCES.md) for every game and add-on and its status.

## Features

- **Multi-game support**: Extract data from all major Fallout games
- **Automatic extraction**: Locates and extracts game data from Steam installations
- **Manual definitions**: Support for TV show and board game locations via JSON
- **Tiled map generation**: Produces industry-standard Tiled map files
- **Georeferencing**: Maps game coordinates to real-world latitude/longitude
- **Marker extraction**: Extracts locations, POIs, and other map markers
- **Terrain processing**: Handles heightmaps and terrain data

## Installation

### Prerequisites

- Rust (edition 2024)
- .NET 9 SDK, for the Mutagen helper in `tools/mutagen_dump` (`build.rs` uses `$DOTNET_ROOT/dotnet` if set)
- Fallout game installations (automatically detected via Steam)

### Building

```bash
cargo build --release
```

## Usage

```bash
cargo run --release
```

The tool will:
1. Locate installed Fallout games
2. Extract map data and markers
3. Generate Tiled maps in the `output/maps/` directory
4. Create tilesets in the `output/tiles/` directory

## Project Structure

- `src/extractors/` - Game-specific data extractors
- `src/map_gen.rs` - Tiled map generation
- `src/tile_gen.rs` - Tileset generation
- `src/rubbersheeting.rs` - Georeferencing and coordinate transformation
- `src/schema.rs` - Data structures for markers and map data
- `input/config/` - Configuration files including anchor points

## Output

- **Maps**: `output/maps/*.tmx` - Tiled map files
- **Tilesets**: `output/tiles/*.tsx` - Tileset definitions
- **Logs**: `output/fallout_map.log` - Processing logs

## Configuration

Map anchor points and configuration are stored in `input/config/paths.json`.

## License

MIT License - See LICENSE file for details

## Contributing

Contributions are welcome! This project is in active development.
