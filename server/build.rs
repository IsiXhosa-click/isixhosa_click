use std::process::Command;

fn get_last_changed(path: &str) -> String {
    let output = Command::new("bash")
        .args(&["-c", &format!("stat -c %Y {path} | sort -n | head -1")])
        .output()
        .unwrap();

    String::from_utf8(output.stdout).unwrap()
}

fn main() {
    println!("cargo:rerun-if-changed=static/");
    println!(
        "cargo:rustc-env=STATIC_LAST_CHANGED={}",
        get_last_changed("static")
    );
    println!(
        "cargo:rustc-env=STATIC_BIN_FILES_LAST_CHANGED={}",
        get_last_changed("static/**/*.{png,svg,woff2,ico}")
    );
}
