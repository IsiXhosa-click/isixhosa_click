use std::process::Command;

fn get_hash_for_path(path: &str) -> String {
    let output = Command::new("bash")
        .args(&["-c", &format!("git log -n 1 --pretty=format:%H {}", path)])
        .output()
        .unwrap();

    String::from_utf8(output.stdout).unwrap()
}

fn main() {
    println!("cargo:rerun-if-changed=static/");
    println!("cargo:rustc-env=GIT_HASH={}", get_hash_for_path("static"));
    println!(
        "cargo:rustc-env=GIT_BIN_FILES_HASH={}",
        get_hash_for_path("static/**/*.{png,svg,woff2,ico}")
    );
}
