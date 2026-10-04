use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Builds the C# Mutagen helper (`tools/mutagen_dump`) that decodes Bethesda plugins.
/// Mutagen 0.54 needs the .NET 9 SDK: `$DOTNET_ROOT/dotnet` is preferred over the `dotnet` on PATH.
fn main() {
    let helper = Path::new("tools/mutagen_dump");
    println!("cargo:rerun-if-changed={}", helper.join("mutagen_dump.csproj").display());
    for f in std::fs::read_dir(helper).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "cs")) {
        println!("cargo:rerun-if-changed={}", f.display());
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

    fo76utils();
}

/// fo76utils (MIT; https://github.com/fo76utils/fo76utils) reads Fallout 76, which Mutagen does not:
/// fetched at a pinned commit into `.cache/tools/fo76utils` and its `esmdump` and `btddump` tools built
/// with scons (needs git, scons and a C++ compiler).
fn fo76utils() {
    const REPO: &str = "https://github.com/fo76utils/fo76utils.git";
    const COMMIT: &str = "c52da18cbe0b7529673bb1c373a89ee5efa37260";
    let dir = std::env::current_dir().unwrap().join(".cache/tools/fo76utils");
    let run = |cmd: &mut Command, what: &str| {
        let ok = cmd.status().map(|s| s.success()).unwrap_or(false);
        if !ok { panic!("fo76utils: {} failed (needs git, scons and a C++ compiler)", what); }
    };
    let at_commit = Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "HEAD"]).output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == COMMIT);
    if !at_commit {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        run(Command::new("git").args(["clone", "--quiet", REPO]).arg(&dir), "git clone");
        run(Command::new("git").arg("-C").arg(&dir).args(["checkout", "--quiet", COMMIT]), "git checkout");
        run(Command::new("git").arg("-C").arg(&dir).args(["submodule", "update", "--init", "--quiet"]), "git submodule update");
    }
    let tools = ["esmdump", "btddump"];
    if !tools.iter().all(|t| dir.join(t).exists()) {
        run(Command::new("scons").current_dir(&dir).arg("-j8").args(tools), "scons build");
    }
    for t in tools {
        println!("cargo:rustc-env=FO76UTILS_{}={}", t.to_uppercase(), dir.join(t).display());
    }
}
