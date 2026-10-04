// The Fallout 4 reader: the same output as Fallout3.cs from Mutagen.Bethesda.Fallout4's records.
using System.Text.Json;
using Mutagen.Bethesda;
using Mutagen.Bethesda.Environments;
using Mutagen.Bethesda.Fallout4;
using Mutagen.Bethesda.Plugins;
using Mutagen.Bethesda.Plugins.Cache;
using Mutagen.Bethesda.Plugins.Implicit;
using Mutagen.Bethesda.Plugins.Order;

static class Fallout4Dump
{
    public static int Run(string dataDir, string worldEdid, string outPath)
    {
        var release = GameRelease.Fallout4;
        // The official masters, then the Creation Club plugins the game lists (Fallout4.ccc, next to Data).
        var ccc = Path.Combine(Path.GetDirectoryName(Path.GetFullPath(dataDir).TrimEnd('/'))!, "Fallout4.ccc");
        var creations = File.Exists(ccc) ? File.ReadAllLines(ccc).Select(l => l.Trim()).Where(l => l.Length > 0) : [];
        var masters = Implicits.Get(release).BaseMasters
            .Concat(creations.Select(c => ModKey.FromFileName(c)))
            .Where(mk => File.Exists(Path.Combine(dataDir, mk.FileName)))
            .Distinct()
            .ToArray();
        using var env = GameEnvironment.Typical.Builder<IFallout4Mod, IFallout4ModGetter>(release)
            .WithTargetDataFolder(dataDir)
            .WithLoadOrder(masters)
            .Build();
        var lc = env.LinkCache;
        var order = env.LoadOrder.PriorityOrder;


        var world = order.Worldspace().WinningOverrides().FirstOrDefault(w => w.EditorID == worldEdid);
        if (world == null)
        {
            Console.Error.WriteLine($"worldspace {worldEdid} not found; available: {string.Join(", ", order.Worldspace().WinningOverrides().Select(w => w.EditorID))}");
            return 1;
        }

        bool InWorld<T>(IModContext<IFallout4Mod, IFallout4ModGetter, T, T> ctx) where T : class =>
            ctx.TryGetParent<IWorldspaceGetter>(out var w) && w.FormKey == world.FormKey;
        string Key(FormKey fk) => fk.ToString();

        // Cells: grid position and water.
        var cells = new Dictionary<FormKey, Dictionary<string, object?>>();
        foreach (var ctx in order.Cell().WinningContextOverrides(lc))
        {
            if (!InWorld(ctx) || ctx.Record.Grid == null) continue;
            var c = ctx.Record;
            cells[c.FormKey] = new()
            {
                ["x"] = c.Grid.Point.X,
                ["y"] = c.Grid.Point.Y,
                ["has_water"] = c.Flags.HasFlag(Cell.Flag.HasWater),
                ["water_height"] = c.WaterHeight,
            };
        }

        // Landscape of each cell: absolute vertex heights (33x33, row-major from the south-west corner) and layers.
        foreach (var ctx in order.Landscape().WinningContextOverrides(lc))
        {
            if (!ctx.TryGetParent<ICellGetter>(out var cell) || !cells.TryGetValue(cell.FormKey, out var c)) continue;
            var land = ctx.Record;
            if (land.VertexHeightMap is { } vhgt)
            {
                // VHGT stores deltas: the first column accumulates down the rows, the others along each row; unit = 8.
                var heights = new int[33 * 33];
                float rowStart = vhgt.Offset * 8;
                for (int y = 0; y < 33; y++)
                {
                    rowStart += vhgt.HeightMap[0, y] * 8;
                    float h = rowStart;
                    heights[y * 33] = (int)MathF.Round(h);
                    for (int x = 1; x < 33; x++)
                    {
                        h += vhgt.HeightMap[x, y] * 8;
                        heights[y * 33 + x] = (int)MathF.Round(h);
                    }
                }
                c["heights"] = heights;
            }
            c["layers"] = land.Layers.Select(l => new Dictionary<string, object?>
            {
                ["quadrant"] = (int)l.Header!.Quadrant,
                ["layer"] = l.Header.LayerNumber,
                ["texture"] = l.Header.Texture.IsNull ? null : Key(l.Header.Texture.FormKey),
                // Alpha layers: (vertex index within the 17x17 quadrant, opacity); base layers have none.
                // Fallout 4 keeps the alpha data raw: 8 bytes per vertex (u16 position, u16 unused, f32 opacity).
                ["alpha"] = l is IAlphaLayerGetter a ? AlphaData(a.AlphaLayerData) : null,
            }).ToArray();
        }

        static object[][] AlphaData(Noggog.ReadOnlyMemorySlice<byte>? raw)
        {
            if (raw is not { } bytes) return [];
            var span = bytes.Span;
            return Enumerable.Range(0, span.Length / 8).Select(i => new object[] {
                BitConverter.ToUInt16(bytes.Span.Slice(i * 8, 2)),
                MathF.Round(BitConverter.ToSingle(bytes.Span.Slice(i * 8 + 4, 4)), 3),
            }).ToArray();
        }

        // Land textures referenced anywhere.
        var textures = new Dictionary<string, object?>();
        foreach (var t in order.LandscapeTexture().WinningOverrides())
        {
            var set = t.TextureSet.TryResolve(lc);
            textures[Key(t.FormKey)] = new Dictionary<string, object?>
            {
                ["edid"] = t.EditorID,
                ["texture"] = set?.Diffuse,
                ["material"] = t.MaterialType.TryResolve(lc)?.EditorID,
                ["grasses"] = t.Grasses.Select(g => g.TryResolve(lc)?.EditorID).Where(e => e != null).ToArray(),
            };
        }

        // Child worldspaces (e.g. Freeside, the Strip): only their map markers are taken, in their own
        // coordinates, with the ONAM map offset that places them on this worldspace's map.
        var children = order.Worldspace().WinningOverrides()
            .Where(w => w.Parent != null && w.Parent.Worldspace.FormKey == world.FormKey)
            .ToDictionary(w => w.FormKey);

        // Map markers, radiation sources (placed objects with an XRAD value) and doors leading elsewhere.
        var markers = new List<object>();
        var childMarkers = new List<object>();
        var radiation = new List<object>();
        var doors = new List<float[]>();
        var statics = new List<object>();
        var waterPlanes = new List<object>();
        var volumes = new List<object>();
        foreach (var ctx in order.PlacedObject().WinningContextOverrides(lc))
        {
            if (!ctx.TryGetParent<IWorldspaceGetter>(out var ws)) continue;
            var r = ctx.Record;
            Noggog.P3Float? pos = r.Position;
            Dictionary<string, object?>? Marker(IPlacedObjectMapMarkerGetter m) => new()
            {
                ["name"] = m.Name?.String,
                ["edid"] = r.EditorID,
                ["type"] = m.Type.ToString(),
                ["flags"] = (int)m.Flags,
                ["reputation"] = null, // Fallout 4 has no faction reputations
                ["x"] = pos.Value.X,
                ["y"] = pos.Value.Y,
            };
            if (children.TryGetValue(ws.FormKey, out var child))
            {
                if (r.MapMarker is { } cm)
                {
                    var entry = Marker(cm)!;
                    entry["worldspace"] = child.EditorID;
                    entry["map_scale"] = child.WorldMapOffsetScale;
                    entry["map_offset"] = new[] { child.WorldMapCellOffset.X, child.WorldMapCellOffset.Y };
                    childMarkers.Add(entry);
                }
                continue;
            }
            if (ws.FormKey != world.FormKey) continue;
            if (r.MapMarker is { } m) markers.Add(Marker(m)!);
            if (r.TeleportDestination != null) doors.Add(new[] { pos.Value.X, pos.Value.Y });
            // Statics and trees with their footprint (object bounds x scale, in square units) and mesh path;
            // which meshes count as buildings or vegetation is decided in Rust.
            (IObjectBoundsGetter? Bounds, string? Model) shape = default;
            if (r.Base.TryResolve<IStaticGetter>(lc, out var st)) shape = (st.ObjectBounds, st.Model?.File);
            else if (r.Base.TryResolve<ITreeGetter>(lc, out var tree)) shape = (tree.ObjectBounds, tree.Model?.File);
            // A static collection's own mesh is an anonymous precombined one: it takes the mesh of its part
            // with the largest footprint over all its placements, so Rust classifies it by what it holds.
            else if (r.Base.TryResolve<IStaticCollectionGetter>(lc, out var scol))
            {
                var dominant = scol.Parts
                    .Select(p => (Static: p.Static.TryResolve<IStaticGetter>(lc, out var st) ? st : null, Placements: p.Placements))
                    .Where(p => p.Static?.ObjectBounds is { } && !string.IsNullOrEmpty(p.Static.Model?.File))
                    .DefaultIfEmpty()
                    .MaxBy(p => p.Placements == null ? 0f : p.Placements.Sum(pl => Math.Abs(p.Static!.ObjectBounds.Second.X - p.Static.ObjectBounds.First.X)
                        * Math.Abs(p.Static.ObjectBounds.Second.Y - p.Static.ObjectBounds.First.Y) * pl.Scale * pl.Scale));
                shape = (scol.ObjectBounds, dominant.Static?.Model?.File ?? scol.Model?.File);
            }
            if (shape.Bounds is { } ob && !string.IsNullOrEmpty(shape.Model))
            {
                var scale = r.Scale ?? 1f;
                var area = Math.Abs(ob.Second.X - ob.First.X) * Math.Abs(ob.Second.Y - ob.First.Y) * scale * scale;
                if (area > 0) statics.Add(new Dictionary<string, object> { ["x"] = pos.Value.X, ["y"] = pos.Value.Y, ["area"] = area, ["model"] = shape.Model });
            }
            // Placed water planes (activators with a water type, or placeable water): centre, height, z
            // rotation (radians) and half extents (object bounds x scale).
            IObjectBoundsGetter? waterBounds = null;
            if (r.Base.TryResolve<IActivatorGetter>(lc, out var act) && !act.WaterType.IsNull) waterBounds = act.ObjectBounds;
            if (waterBounds != null)
            {
                var scale = r.Scale ?? 1f;
                waterPlanes.Add(new Dictionary<string, object>
                {
                    ["x"] = pos.Value.X, ["y"] = pos.Value.Y, ["z"] = pos.Value.Z,
                    ["rotation"] = r.Rotation.Z,
                    ["half_x"] = Math.Abs(waterBounds.Second.X - waterBounds.First.X) * scale / 2,
                    ["half_y"] = Math.Abs(waterBounds.Second.Y - waterBounds.First.Y) * scale / 2,
                });
            }
            // Primitive volumes (trigger boxes and spheres): base, centre, z rotation and half extents.
            if (r.Primitive is { } prim)
            {
                var scale = r.Scale ?? 1f;
                volumes.Add(new Dictionary<string, object?>
                {
                    ["base"] = r.Base.TryResolve(lc)?.EditorID,
                    ["x"] = pos.Value.X, ["y"] = pos.Value.Y,
                    ["rotation"] = r.Rotation.Z,
                    ["half_x"] = prim.Bounds.X * scale, ["half_y"] = prim.Bounds.Y * scale,
                    ["shape"] = prim.Type.ToString(),
                });
            }
        }
        // Radiation: Fallout 4 places it as hazards (their radius, and the effect they apply).
        // (Mutagen's placed-hazard enumeration mis-casts other placed traps, so the cells' references are
        // filtered instead.)
        var worldCells = order.Cell().WinningContextOverrides(lc).Where(InWorld).Select(c => c.Record)
            .Concat(world.TopCell is { } top ? [top] : []);
        foreach (var h in worldCells.SelectMany(c => c.Persistent.Concat(c.Temporary)).OfType<IPlacedHazardGetter>())
        {
            var hazard = h.Hazard.TryResolve(lc);
            if (hazard == null) continue;
            radiation.Add(new Dictionary<string, object?>
            {
                ["base"] = hazard.EditorID,
                ["effect"] = hazard.Effect.TryResolve(lc)?.EditorID,
                ["radiation"] = null,
                ["radius"] = hazard.Radius * 64f / 3f * (h.Scale ?? 1f), // in feet; a foot is 64/3 units
                ["x"] = h.Position.X,
                ["y"] = h.Position.Y,
            });
        }

        // Regions of this worldspace (weather, sound and named areas such as the Glowing Sea): their
        // polygons in world units.
        var regions = order.Region().WinningOverrides()
            .Where(rg => rg.Worldspace.FormKey == world.FormKey && rg.RegionAreas.Count > 0)
            .Select(rg => new Dictionary<string, object?>
            {
                ["edid"] = rg.EditorID,
                ["areas"] = rg.RegionAreas.Select(ar => ar.RegionPointListData.Select(pt => new[] { pt.X, pt.Y }).ToArray()).ToArray(),
            }).ToList();

        var landData = world.LandDefaults;
        var mapData = world.MapData;
        var output = new Dictionary<string, object?>
        {
            ["load_order"] = masters.Select(m => m.FileName.String).ToArray(),
            ["worldspace"] = new Dictionary<string, object?>
            {
                ["edid"] = world.EditorID,
                ["name"] = world.Name?.String,
                ["default_land_height"] = landData?.DefaultLandHeight,
                ["default_water_height"] = landData?.DefaultWaterHeight,
                ["bounds_min"] = world.ObjectBoundsMin is { } bmin ? new[] { bmin.X, bmin.Y } : null,
                ["bounds_max"] = world.ObjectBoundsMax is { } bmax ? new[] { bmax.X, bmax.Y } : null,
                ["map_nw_cell"] = mapData is null ? null : new[] { (int)mapData.NorthwestCellCoords.X, (int)mapData.NorthwestCellCoords.Y },
                ["map_se_cell"] = mapData is null ? null : new[] { (int)mapData.SoutheastCellCoords.X, (int)mapData.SoutheastCellCoords.Y },
            },
            ["markers"] = markers,
            ["child_markers"] = childMarkers,
            ["doors"] = doors,
            ["statics"] = statics,
            ["water_planes"] = waterPlanes,
            ["volumes"] = volumes,
            ["radiation"] = radiation,
            ["land_textures"] = textures,
            ["cells"] = cells.Values.ToArray(),
            ["regions"] = regions,
        };
        Output.Write(outPath, output);
        Console.Error.WriteLine($"{world.EditorID}: {markers.Count} markers, {cells.Count} cells, {textures.Count} land textures, {radiation.Count} radiation sources; load order {string.Join(", ", masters.Select(m => m.FileName.String))}");
        return 0;
    }
}
