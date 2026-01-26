use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=ui/");
    // Install ui dependencies
    let output = Command::new("npm")
        .args(&["install"])
        .current_dir("./ui")
        .output()
        .expect("Failed to run npm");
    if !output.status.success() {
        panic!("'npm install' failed")
    }

    // Build ui
    let output = Command::new("npm")
        .args(&["run", "build"])
        .current_dir("./ui")
        .output()
        .expect("Failed to run npm");
    if !output.status.success() {
        panic!("'npm run build' failed")
    }
}
