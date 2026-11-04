//! Embeds `samo-clips.manifest` in the helper on Windows (MSVC's linker): Windows' modern controls and DPI awareness.
//! Nothing elsewhere.

fn main() {
    println!("cargo:rerun-if-changed=samo-clips.manifest");
    let is = |key: &str, value: &str| std::env::var(key).is_ok_and(|v| v == value);
    if is("CARGO_CFG_TARGET_OS", "windows") && is("CARGO_CFG_TARGET_ENV", "msvc") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let manifest = std::path::Path::new(&dir).join("samo-clips.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
