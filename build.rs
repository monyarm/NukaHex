use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Builds the C# Mutagen helper (`tools/mutagen_dump`) that decodes Bethesda plugins.
/// Mutagen 0.54 needs the .NET 9 SDK: `$DOTNET_ROOT/dotnet` is preferred over the `dotnet` on PATH.
fn main() {
    let helper = Path::new("tools/mutagen_dump");
    for f in ["Program.cs", "mutagen_dump.csproj"] {
        println!("cargo:rerun-if-changed={}", helper.join(f).display());
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DOTNET_ROOT");

    let dotnet = env::var("DOTNET_ROOT").ok().map(|r| Path::new(&r).join("dotnet")).filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("dotnet"));
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("mutagen_dump");
    let status = Command::new(&dotnet)
        .args(["build", "-c", "Release", "--nologo", "-v", "q", "-o"]).arg(&out)
        .arg(helper.join("mutagen_dump.csproj"))
        .status()
        .unwrap_or_else(|e| panic!("cannot run {} ({}): the .NET 9 SDK is needed to build tools/mutagen_dump", dotnet.display(), e));
    if !status.success() {
        panic!("building tools/mutagen_dump failed; it needs the .NET 9 SDK (set DOTNET_ROOT if `dotnet` on PATH is older)");
    }
    println!("cargo:rustc-env=MUTAGEN_DOTNET={}", dotnet.display());
    println!("cargo:rustc-env=MUTAGEN_DUMP_DLL={}", out.join("mutagen_dump.dll").display());
}
