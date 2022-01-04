use std::process::Command;

fn main() {
    let output = Command::new("git")
        .args(&["log", "-n", "1", "--pretty=format:%H", "static"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
