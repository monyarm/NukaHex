// Dumps one worldspace of a Fallout 3 / New Vegas / Fallout 4 install as JSON for the Rust extractor:
// map markers (and those of child worldspaces), per-cell landscape (absolute heights + texture layers),
// land textures, placed statics and trees, water planes, primitive volumes, doors and radiation sources.
// Raw decoded data only; all classification happens in Rust.
//
// Usage: mutagen_dump <Fallout3|FalloutNV|Fallout4> <data dir> <worldspace EditorID> <out.json>
// Always loads every official master of the game present in the data folder (base game + all DLC),
// in the game's implicit order, and reports winning overrides.

using System.Text.Json;
using Mutagen.Bethesda;

if (args.Length != 4)
{
    Console.Error.WriteLine("usage: mutagen_dump <Fallout3|FalloutNV|Fallout4> <data dir> <worldspace EditorID> <out.json>");
    return 2;
}
var release = Enum.Parse<GameRelease>(args[0]);
return release == GameRelease.Fallout4
    ? Fallout4Dump.Run(args[1], args[2], args[3])
    : Fallout3Dump.Run(release, args[1], args[2], args[3]);

static class Output
{
    public static void Write(string path, Dictionary<string, object?> output)
    {
        using var f = File.Create(path);
        JsonSerializer.Serialize(f, output);
    }
}
