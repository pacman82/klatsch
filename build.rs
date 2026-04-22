use std::{fs, path::PathBuf, process::Command};

use walkdir::WalkDir;

/// File/directory names excluded from the UI fingerprint walk. `.fingerprint` itself is excluded
/// to avoid a self-reference (writing it would otherwise change the hash). `build/` is *not*
/// excluded: we want the fingerprint to include the build output so that tampering with it (or a
/// botched previous build) forces a rebuild.

/// Directories that are generated during the build need not be hashed as inputs. The fingerprint
/// must be reproducible with the contents from `cargo package`. We do not skip `build` because we
/// want to rebuild in case something messed with the output.
const SKIP_PATHS: &[&str] = &["node_modules", ".svelte-kit", ".fingerprint"];

fn main() {
    // No `rerun-if-changed` directives — this build script runs every time. It performs its own
    // staleness check to avoid spawning npm when nothing changed.

    let current_fingerprint = compute_fingerprint();
    let stored_fingerprint = load_fingerprint();
    let ui_build_present = PathBuf::from("ui").join("build").is_dir();

    if ui_build_present && current_fingerprint == stored_fingerprint {
        return;
    }

    // Trust an externally-produced build (e.g. from a CI pre-build step): if `ui/build` is present
    // but no fingerprint has been stored yet, record the current source fingerprint and avoid
    // running npm again. The crate tarball then ships the fingerprint alongside the build so that
    // verification reproduces a skip instead of trying to rebuild.
    if ui_build_present && stored_fingerprint.is_empty() {
        save_fingerprint(current_fingerprint);
        return;
    }

    build_ui();
    save_fingerprint(compute_fingerprint());
}

fn build_ui() {
    execute_npm_command(&["install"]);
    execute_npm_command(&["run", "build"]);
}

fn save_fingerprint(fingerprint: String) {
    fs::write(fingerprint_path(), fingerprint).expect("failed to write UI fingerprint")
}

fn load_fingerprint() -> String {
    fs::read_to_string(fingerprint_path()).unwrap_or_default()
}

fn fingerprint_path() -> PathBuf {
    PathBuf::from("ui").join(".fingerprint")
}

/// Produces a single hash covering all `ui/` source files. Any change to source files or config
/// produces a different fingerprint; build outputs and the fingerprint file itself are excluded so
/// the hash represents source state only.
fn compute_fingerprint() -> String {
    let mut hasher = blake3::Hasher::new();

    for entry in WalkDir::new("ui")
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !SKIP_PATHS.contains(&e.file_name().to_string_lossy().as_ref()))
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
