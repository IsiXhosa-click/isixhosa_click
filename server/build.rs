use std::process::Command;

fn get_last_changed(path: &str) -> String {
    let output = Command::new("bash")
        // Taken from a comment in https://stackoverflow.com/a/7448828
        .args([
            "-c",
            &format!(
                "find {path} -type f -print0 | xargs -0 stat --format '%Y' |\
                 sort -nr | cut -d: -f2- | head -1"
            ),
        ])
        .output()
        .unwrap();

    String::from_utf8(output.stdout).unwrap()
}

fn main() {
    println!("cargo:rerun-if-changed=static/");
    println!(
        "cargo:rustc-env=STATIC_LAST_CHANGED={}",
        get_last_changed("static").trim()
    );
    println!(
        "cargo:rustc-env=STATIC_BIN_FILES_LAST_CHANGED={}",
        get_last_changed("static/**/*.{png,svg,woff2,ico}").trim()
    );
}
