//! The helper carries its manifest (Common Controls 6, DPI-aware): without it, the list draws as Windows 2000 did and
//! its search field lacks Ctrl+A.
#![cfg(all(windows, target_env = "msvc"))]

#[test]
fn the_helper_asks_for_modern_controls() -> Result<(), String> {
    let exe = std::fs::read(env!("CARGO_BIN_EXE_samo-clips")).map_err(|e| e.to_string())?;
    let has = |needle: &[u8]| exe.windows(needle.len()).any(|w| w == needle);
    assert!(
        has(b"Microsoft.Windows.Common-Controls"),
        "no Common Controls 6"
    );
    assert!(has(b"dpiAware"), "not DPI-aware");
    Ok(())
}
