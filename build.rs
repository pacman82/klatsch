use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=ui/");
    // Install ui dependencies
    execute_npm_command(&["install"]);
    // Build the ui
    execute_npm_command(&["run", "build"]);
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
