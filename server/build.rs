use std::process::Command;

fn get_last_changed(path: &str, name_pat: &str) -> String {
    let output = Command::new("bash")
        // Taken from a comment in https://stackoverflow.com/a/7448828
        .args([
            "-c",
            &format!(
                "find {path} -type f {name_pat} -print0 | xargs -0 stat --format '%Y' |\
                 sort -nr | cut -d: -f2- | head -1"
            ),
        ])
        .output()
        .unwrap();

    String::from_utf8(output.stdout).unwrap()
}

fn main() {
    println!("cargo:rerun-if-changed=static/");

    let static_exts = ["png", "svg", "woff2", "ico"]
        .map(|ext| format!("-name '*.{ext}'"))
        .join(" -o ");
    let static_pat = format!("\\( {static_exts} \\)");

    println!(
        "cargo:rustc-env=STATIC_LAST_CHANGED={}",
        get_last_changed("static", &format!("! {static_pat}")).trim()
    );

    println!(
        "cargo:rustc-env=STATIC_BIN_FILES_LAST_CHANGED={}",
        get_last_changed("static", &static_pat).trim()
    );
}
