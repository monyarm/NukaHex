# 🗺️ Project: Fallout Tiled Map Generator & Georeferencer

## 1. Executive Summary
This project aims to build a tool that generates **Tiled** maps (`.tmx`) and tilesets (`.tsx`) to visualize the Fallout world. The ultimate goal is to create a unified map of the United States by "un-rubbersheeting" game maps—correcting their distorted scales and positions using real-world anchor points—and overlaying them onto accurate real-world data (borders, topography).

The project will proceed in phases, starting with a generic generator, moving to the simpler Fallout 1/2 maps, and finally addressing the complex georeferencing required for a cohesive US map.

## 2. Architecture & Tech Stack

- **Core Generator**: Python (ideal for image processing `Pillow`, GIS data handling `GDAL`/`Rasterio`, and XML/JSON).
- **Map Format**: Tiled (`.tmx` for maps, `.tsx` for tilesets).
- **Data Sources**:
    - **Game Data**: Extracted markers, heightmaps, and terrain types.
    - **Real World Data**: GIS vectors for State/Country borders, coastlines (e.g., Natural Earth Data or OpenStreetMap).
    - **Anchor Points**: A JSON database mapping "Game Coordinates" <-> "Real World Lat/Lon".

## 3. Implementation Plan

### Phase 1: Generic Proof of Concept (The "Hello World" Map)
**Goal**: Prove the pipeline by generating a valid Tiled map from arbitrary dummy data.

1.  **Tileset Generator (`tsx_builder`)**:
    - Scan a `./tiles/` directory (containing `grass.png`, `water.png`, `?_land.png`, `?_sea.png`).
    - Generate `generic.tsx`.
2.  **Simple Map Generator**:
    - Create a 100x100 grid.
    - Procedurally fill it:
        - Circle of "Land" in the center.
        - "Water" on edges.
        - Random "Markers" placed on top.
    - Export `test_map.tmx`.
3.  **Validation**: Open `test_map.tmx` in Tiled to confirm layers and tiles load correctly.

### Phase 2: Fallout 1 & 2 Integration (Classic Maps)
**Goal**: Handle actual game data from the classic 2D games, which are simpler than the 3D titles.

1.  **Data Extraction**:
    - FO1/FO2 world maps are grid-based/image-based. extracting the "squares" or the base world map image.
    - Extract locations (Vault 13, Shady Sands, etc.) as markers.
2.  **Tiled Conversion**:
    - Convert the game's grid directly to Tiled layers.
    - Place markers on an Object Layer.

### Phase 3: The "Un-Rubbersheeting" (Georeferencing)
**Goal**: Correct the distorted ("rubbersheeted") game maps to fit real-world geography.

1.  **Anchor Point Database**:
    - Create a list of "Real Map Markers" for each game.
    - *Example*: `The Glow` (West Tek) -> Real World Lat/Lon of crater/specific facility.
    - *Example*: `Las Vegas Strip` -> Real Coordinates.
2.  **Transformation Logic**:
    - Use the Anchor Points to calculate a transformation matrix (Affine or Thin Plate Spline) to warp the game coordinates to Real World Lat/Lon.
    - "Un-distort" the game markers and terrain patches.
3.  **Real World Base Layer**:
    - Render State and Country borders (US States, Canada/Mexico borders).
    - Render verified coastlines.
4.  **The "Unknown" Zones**:
    - Any map tile *not* covered by valid game data or real-world confirmed locations is filled with:
        - `?_land` tile (for generic landmass).
        - `?_sea` tile (for oceans/lakes).

### Phase 4: Final Assembly (The Master US Map)
**Goal**: Combine all processed maps into one massive master file.

- **Stacking Order**:
    1.  **Base**: Real-world borders/coastlines (Vector layer or rasterized lines).
    2.  **Fill**: "?" tiles for empty space.
    3.  **Overlay**: The "Un-rubbersheeted" Game Maps (FO1, FO2, NV, 3, 4, 76) placed in their correct geographic positions.
    4.  ** markers**: All POIs with their corrected coordinates.
