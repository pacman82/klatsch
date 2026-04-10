use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use walkdir::WalkDir;

/// Directories that are generated during the build need not be hashed as inputs, we skip them to be
/// faster. Especially `node_modules` is expensive to hash. We do not skip `build` because we want
/// to rebuild in case something messes with the output.
const SKIP_DIRS: &[&str] = &["node_modules", ".svelte-kit"];

fn main() {
    // No `rerun-if-changed` directives — this build script runs every time. It performs its own
    // staleness check to avoid spawning npm when nothing changed.

    let current_fingerprint = compute_fingerprint();
    let previous_fingerprint = load_fingerprint();

    if current_fingerprint == previous_fingerprint {
        return;
    }

    build_ui();

    // Recompute after building — the build output is part of the fingerprint.
    let new_fingerprint = compute_fingerprint();
    save_fingerprint(new_fingerprint);
}

fn build_ui() {
    execute_npm_command(&["install"]);
    execute_npm_command(&["run", "build"]);
}

fn save_fingerprint(new_fingerprint: String) {
    fs::write(&fingerprint_path(), new_fingerprint).expect("failed to write UI fingerprint")
}

fn load_fingerprint() -> String {
    fs::read_to_string(&fingerprint_path()).unwrap_or_default()
}

fn fingerprint_path() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").unwrap()).join("ui.fingerprint")
}

/// Produces a single hash covering all files in `ui/`, excluding generated directories. Any change
/// to source files, config, or build output — including addition or removal of files — produces a
/// different fingerprint.
fn compute_fingerprint() -> String {
    let mut hasher = blake3::Hasher::new();

    for entry in WalkDir::new("ui")
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            !e.file_type().is_dir()
                || !SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref())
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        hasher.update(path.to_string_lossy().as_bytes());
        if let Ok(contents) = fs::read(path) {
            hasher.update(&contents);
        }
    }

    hasher.finalize().to_hex().to_string()
}

#[cfg(not(windows))]
fn execute_npm_command(args: &[&str]) {
    let output = Command::new("npm")
        .args(args)
        .current_dir("./ui")
        .output()
        .expect("Failed to run npm");
    if !output.status.success() {
        panic!(
            "npm {} failed:\nstdout: {}\nstderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[cfg(windows)]
fn execute_npm_command(args: &[&str]) {
    let command = String::from("npm ") + &args.join(" ");
    let output = Command::new("powershell")
        .args(&["-Command", &command])
        .current_dir("./ui")
        .output()
        .expect("Failed to run npm");
    if !output.status.success() {
        panic!(
            "npm {} failed:\nstdout: {}\nstderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
