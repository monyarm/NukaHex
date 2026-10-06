// The Fallout 3 / New Vegas reader (Mutagen.Bethesda.Fallout3 covers both games).
using System.Text.Json;
using Mutagen.Bethesda;
using Mutagen.Bethesda.Environments;
using Mutagen.Bethesda.Fallout3;
using Mutagen.Bethesda.Plugins;
using Mutagen.Bethesda.Plugins.Cache;
using Mutagen.Bethesda.Plugins.Implicit;
using Mutagen.Bethesda.Plugins.Order;

static class Fallout3Dump
{
    public static int Run(GameRelease release, string dataDir, string worldEdid, string outPath, string[] plugins, bool addedOnly)
    {
        var masters = Implicits.Get(release).BaseMasters
            .Where(mk => File.Exists(Path.Combine(dataDir, mk.FileName)))
            .Concat(plugins.Select(p => ModKey.FromFileName(p)))
            .ToArray();
        var pluginKeys = plugins.Select(p => ModKey.FromFileName(p)).ToHashSet();
        using var env = GameEnvironment.Typical.Builder<IFallout3Mod, IFallout3ModGetter>(release)
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

        bool InWorld<T>(IModContext<IFallout3Mod, IFallout3ModGetter, T, T> ctx) where T : class =>
            ctx.TryGetParent<IWorldspaceGetter>(out var w) && w.FormKey == world.FormKey;
        string Key(FormKey fk) => fk.ToString();

        // Cells: grid position and water.
        var cells = new Dictionary<FormKey, Dictionary<string, object?>>();
        var newCells = new HashSet<FormKey>(); // created by the given plugins
        foreach (var ctx in order.Cell().WinningContextOverrides(lc))
        {
            if (!InWorld(ctx) || ctx.Record.Grid == null) continue;
            var c = ctx.Record;
            if (pluginKeys.Contains(c.FormKey.ModKey)) newCells.Add(c.FormKey);
            cells[c.FormKey] = new()
            {
                ["x"] = c.Grid.GridX,
                ["y"] = c.Grid.GridY,
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
                ["alpha"] = l is IAlphaLayerGetter a ? a.AlphaLayerData.Select(d => new object[] { d.Position, MathF.Round(d.Opacity, 3) }).ToArray() : null,
            }).ToArray();
        }

        // --added-only: the cells the plugins add, or whose landscape they reshape (the Wasteland already
        // has cells past its border, the far land its LOD shows, which a mod extending it overrides).
        var addedGrid = new HashSet<(int, int)>();
        if (addedOnly)
        {
            var reshaped = order.Landscape().WinningContextOverrides(lc)
                .Where(ctx => pluginKeys.Contains(ctx.ModKey) && ctx.TryGetParent<ICellGetter>(out _))
                .Select(ctx => { ctx.TryGetParent<ICellGetter>(out var cell); return cell!.FormKey; })
                .ToHashSet();
            foreach (var key in cells.Keys.Where(k => !newCells.Contains(k) && !reshaped.Contains(k)).ToList()) cells.Remove(key);
            foreach (var c in cells.Values) addedGrid.Add(((int)c["x"]!, (int)c["y"]!));
        }

        // Land textures referenced anywhere.
        var textures = new Dictionary<string, object?>();
        foreach (var t in order.LandscapeTexture().WinningOverrides())
        {
            var set = t.TextureSet.TryResolve(lc);
            textures[Key(t.FormKey)] = new Dictionary<string, object?>
            {
                ["edid"] = t.EditorID,
                ["texture"] = set?.BaseImage?.DataRelativePath.Path ?? t.Icons?.LargeIconFilename?.DataRelativePath.Path,
                ["material"] = t.Havok.Material.ToString(),
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
            var pos = r.Placement?.Position;
            if (pos == null) continue;
            Dictionary<string, object?>? Marker(IMapMarkerGetter m) => new()
            {
                ["name"] = m.Name,
                ["edid"] = r.EditorID,
                ["type"] = m.Type?.ToString(),
                ["flags"] = (int)m.Flags,
                ["reputation"] = m.Reputation.TryResolve(lc)?.EditorID,
                ["x"] = pos.Value.X,
                ["y"] = pos.Value.Y,
            };
            if (children.TryGetValue(ws.FormKey, out var child))
            {
                if (r.MapMarker is { } cm)
                {
                    var entry = Marker(cm)!;
                    entry["worldspace"] = child.EditorID;
                    entry["map_scale"] = child.WorldMapScale;
                    entry["map_offset"] = new[] { child.CellXOffset, child.CellYOffset };
                    childMarkers.Add(entry);
                }
                continue;
            }
            if (ws.FormKey != world.FormKey) continue;
            if (r.MapMarker is { } m) markers.Add(Marker(m)!);
            if (addedOnly && !addedGrid.Contains(((int)MathF.Floor(pos.Value.X / 4096), (int)MathF.Floor(pos.Value.Y / 4096)))) continue;
            if (r.TeleportDestination != null) doors.Add(new[] { pos.Value.X, pos.Value.Y });
            // Statics and trees with their footprint (object bounds x scale, in square units) and mesh path;
            // which meshes count as buildings or vegetation is decided in Rust.
            (IObjectBoundsGetter? Bounds, string? Model) shape = default;
            if (r.Base.TryResolve<IStaticGetter>(lc, out var st)) shape = (st.ObjectBounds, st.Model?.File);
            else if (r.Base.TryResolve<ITreeGetter>(lc, out var tree)) shape = (tree.ObjectBounds, tree.Model?.File);
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
            else if (r.Base.TryResolve<IPlaceableWaterGetter>(lc, out var pw)) waterBounds = pw.ObjectBounds;
            if (waterBounds != null)
            {
                var scale = r.Scale ?? 1f;
                waterPlanes.Add(new Dictionary<string, object>
                {
                    ["x"] = pos.Value.X, ["y"] = pos.Value.Y, ["z"] = pos.Value.Z,
                    ["rotation"] = r.Placement!.Rotation.Z,
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
                    ["rotation"] = r.Placement!.Rotation.Z,
                    ["half_x"] = prim.Bounds.X * scale, ["half_y"] = prim.Bounds.Y * scale,
                    ["shape"] = prim.PrimitiveType.ToString(),
                });
            }
            if (r.Radiation is > 0)
            {
                radiation.Add(new Dictionary<string, object?>
                {
                    ["base"] = r.Base.TryResolve(lc)?.EditorID,
                    ["radiation"] = r.Radiation,
                    ["radius"] = r.Radius,
                    ["x"] = pos.Value.X,
                    ["y"] = pos.Value.Y,
                });
            }
        }

        var landData = world.LandData;
        var mapData = world.MapData;
        var output = new Dictionary<string, object?>
        {
            ["load_order"] = masters.Select(m => m.FileName.String).ToArray(),
            ["worldspace"] = new Dictionary<string, object?>
            {
                ["edid"] = world.EditorID,
                ["name"] = world.Name,
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
        };
        Output.Write(outPath, output);
        Console.Error.WriteLine($"{world.EditorID}: {markers.Count} markers, {cells.Count} cells, {textures.Count} land textures, {radiation.Count} radiation sources; load order {string.Join(", ", masters.Select(m => m.FileName.String))}");
        return 0;
    }
}
