use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir).join("fo76utils");

    if !target_dir.exists() {
        // Clone the repository
        let status = Command::new("git")
            .arg("clone")
            .arg("--recurse-submodules")
            .arg("https://github.com/fo76utils/fo76utils.git")
            .arg(&target_dir)
            .status()
            .expect("Failed to execute git clone");

        if !status.success() {
            panic!("Git clone of fo76utils failed");
        }
    } else {
        // Optionally update it
        let _ = Command::new("git")
            .current_dir(&target_dir)
            .arg("pull")
            .status();
    }

    // Build the repository with scons
    let scons_status = Command::new("scons")
        .current_dir(&target_dir)
        .arg("-j4")
        .status()
        .expect("Failed to execute scons. Is it installed?");

    if !scons_status.success() {
        panic!("scons build of fo76utils failed");
    }

    // Inform cargo to pass the binary directory to the application
    println!("cargo:rustc-env=FO76UTILS_BIN_DIR={}", target_dir.display());

    // Tell cargo to rerun this build script if it changes
    println!("cargo:rerun-if-changed=build.rs");
}
