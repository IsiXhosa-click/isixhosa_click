use std::env;
use std::process::{Command, Stdio};

fn get_hash_for_path(path: &str) -> String {
    let output = Command::new("bash")
        .args(&["-c", &format!("git log -n 1 --pretty=format:%H {}", path)])
        .output()
        .unwrap();

    String::from_utf8(output.stdout).unwrap()
}

fn compile_wordle_wasm() {
    let profile = env::var("PROFILE").unwrap();
    let args: &[&str] = match profile.as_str() {
        "release" => &["build", "--release", "--target", "wasm32-unknown-unknown"],
        _ => &["build", "--target", "wasm32-unknown-unknown"]
    };

    Command::new("cargo").stdout(Stdio::inherit()).args(args).current_dir("../wordle").status().unwrap();

    let out = format!("../wordle/target/wasm32-unknown-unknown/{}", profile);
    let wasm_in = format!("{}/isixhosa_wordle.wasm", out);
    let wasm_bg = format!("{}/isixhosa_wordle_bg.wasm", out);
    let wasm_opt = format!("{}/isixhosa_wordle_bg.wasm", out);

    Command::new("wasm-bindgen")
        .args(&["--no-typescript", &wasm_in, "--out-dir", &out, "--target", "web"])
        .stdout(Stdio::inherit())
        .status()
        .unwrap();

    Command::new("wasm-opt")
        .args(&[&wasm_bg, "-Oz", "-o", &wasm_opt])
        .stdout(Stdio::inherit())
        .status()
        .unwrap();
}

fn main() {
    // compile_wordle_wasm();
    println!("cargo:rustc-env=GIT_HASH={}", get_hash_for_path("static"));
    println!(
        "cargo:rustc-env=GIT_BIN_FILES_HASH={}",
        get_hash_for_path("static/**/*.{png,svg,woff2,ico}")
    );
}
