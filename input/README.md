# Manual Extractor

The manual extractor allows you to add custom map markers and terrain by placing JSON files in the `inputs/` directory.

## JSON Format

Each JSON file should have the following structure:

```json
{
  "markers": [
    {
      "name": "Location Name",
      "type": "Vault",
      "lat": 36.5,
      "lon": -120.0
    }
  ],
  "terrain": {
    "type": "circle",
    ...
  }
}
```

### Marker Types

Supported marker types:
- `Vault`
- `City`
- `Cave`
- `MilitaryBase`
- `Special`
- `Unknown`

### Terrain Builders

#### Circle Terrain

Creates a circular area of terrain:

```json
{
  "type": "circle",
  "center_lat": 36.5,
  "center_lon": -120.0,
  "radius_hexes": 5,
  "terrain_type": "Sand"
}
```

#### Grid Terrain

Defines terrain as a 2D grid:

```json
{
  "type": "grid",
  "top_left_lat": 38.0,
  "top_left_lon": -121.0,
  "hex_size_degrees": 0.1,
  "tiles": [
    ["Sand", "Sand", "Mountain"],
    ["Water", "Sand", "Sand"],
    ["Water", "Water", "Mountain"]
  ]
}
```

Terrain types:
- `Sand`
- `Mountain`
- `Water`
- `City`
- `empty` (no tile)

## Examples

See `inputs/example_circle.json` and `inputs/example_grid.json` for working examples.

## Usage

1. Create JSON files in the `inputs/` directory
2. Run the map generator
3. The manual extractor will automatically load and process all `.json` files
