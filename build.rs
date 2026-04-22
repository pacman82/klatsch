use std::{
    env,
    fs::{copy, create_dir_all, read, read_to_string, remove_dir_all, write},
    path::PathBuf,
    process::Command,
};

use walkdir::{DirEntry, WalkDir};

fn main() {
    // No `rerun-if-changed` directives. We perform our own staleness check using fingerprint below.

    let current_fingerprint = compute_fingerprint();
    let previous_fingerprint = load_fingerprint();

    if current_fingerprint == previous_fingerprint {
        return;
    }

    build_ui();

    save_fingerprint(compute_fingerprint());
}

fn build_ui() {
    stage_ui_sources();
    execute_npm_command(&["install"]);
    execute_npm_command(&["run", "build"]);
}

fn save_fingerprint(fingerprint: String) {
    write(fingerprint_path(), fingerprint).expect("failed to write UI fingerprint")
}

fn load_fingerprint() -> String {
    read_to_string(fingerprint_path()).unwrap_or_default()
}

fn fingerprint_path() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set")).join("ui.fingerprint")
}

fn compute_fingerprint() -> String {
    let mut hasher = blake3::Hasher::new();

    for entry in walk_ui_source_files() {
        let path = entry.path();
        hasher.update(path.to_string_lossy().as_bytes());
        if let Ok(contents) = read(path) {
            hasher.update(&contents);
        }
    }

    hasher.finalize().to_hex().to_string()
}

/// Copy UI sources into target
///
/// npm modifies its working directory. E.g. to install dependencies in `node_modules/`. In order to
/// be compatible with `cargo package` we must not pollute the source tree outside of `target/`.
fn stage_ui_sources() {
    let dest = staging_dir();
    if dest.exists() {
        remove_dir_all(&dest).expect("failed to clean UI staging directory");
    }

    for entry in walk_ui_source_files() {
        let src = entry.path();
        let rel = src
            .strip_prefix("ui")
            .expect("walked path always has the `ui` prefix");
        let dst = dest.join(rel);

        if src.is_dir() {
            create_dir_all(&dst).expect("failed to create staging subdir");
        } else {
            if let Some(parent) = dst.parent() {
                create_dir_all(parent).expect("failed to create staging parent dir");
            }
            copy(src, &dst).expect("failed to copy source into staging");
        }
    }
}

/// Staging directory under cargo's `target/`. Relative path — resolves against CWD, which cargo
/// sets to `CARGO_MANIFEST_DIR` during build.rs execution.
fn staging_dir() -> PathBuf {
    PathBuf::from("target").join("ui")
}

fn walk_ui_source_files() -> impl Iterator<Item = DirEntry> {
    /// File/directory names excluded from the UI source fingerprint walk.
    const SKIP_PATHS: &[&str] = &["node_modules", ".svelte-kit", "build"];

    WalkDir::new("ui")
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            !e.file_type().is_dir()
                || !SKIP_PATHS.contains(&e.file_name().to_string_lossy().as_ref())
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
}

#[cfg(not(windows))]
fn execute_npm_command(args: &[&str]) {
    let output = Command::new("npm")
        .args(args)
        .current_dir(staging_dir())
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
        .current_dir(staging_dir())
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
