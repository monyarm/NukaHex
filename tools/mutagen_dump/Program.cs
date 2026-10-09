// Dumps one worldspace of a Fallout 3 / New Vegas / Fallout 4 install as JSON for the Rust extractor:
// map markers (and those of child worldspaces), per-cell landscape (absolute heights + texture layers),
// land textures, placed statics and trees, water planes, primitive volumes, doors and radiation sources.
// Raw decoded data only; all classification happens in Rust.
//
// Usage: mutagen_dump <Fallout3|FalloutNV|Fallout4> <data dir> <worldspace EditorID> <out.json> [--added-only] [plugin...]
// Always loads every official master of the game present in the data folder (base game + all DLC),
// in the game's implicit order, then the given plugins (a mod's), and
// reports winning overrides. --added-only keeps only the cells those plugins add (a mod extending a
// game's worldspace), and the map markers everywhere.

using System.Text.Json;
using Mutagen.Bethesda;

if (args.Length < 4)
{
    Console.Error.WriteLine("usage: mutagen_dump <Fallout3|FalloutNV|Fallout4> <data dir> <worldspace EditorID> <out.json> [--added-only] [plugin...]");
    return 2;
}
var release = Enum.Parse<GameRelease>(args[0]);
var plugins = args[4..].Where(a => a != "--added-only").ToArray();
var addedOnly = args[4..].Contains("--added-only");
return release == GameRelease.Fallout4
    ? Fallout4Dump.Run(args[1], args[2], args[3], plugins, addedOnly)
    : Fallout3Dump.Run(release, args[1], args[2], args[3], plugins, addedOnly);

static class Output
{
    public static void Write(string path, Dictionary<string, object?> output)
    {
        using var f = File.Create(path);
        JsonSerializer.Serialize(f, output);
    }
}
