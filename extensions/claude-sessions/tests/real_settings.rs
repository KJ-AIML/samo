//! R7 against a real `settings.json`, copied: Connect and Plan limits, then Disconnect and Plan limits off, leave
//! the copy byte-for-byte as it was. The real file is only read. Run by hand on a PC with Claude Code:
//!
//!   SAMO_TEST_SETTINGS=~/.claude/settings.json cargo test -p samo-claude --test real_settings -- --ignored

use samo_claude::connection::{self, Places};
use std::fs;
use std::path::PathBuf;

#[test]
#[ignore = "reads the settings.json named by SAMO_TEST_SETTINGS"]
fn a_real_settings_file_comes_back_byte_for_byte() -> Result<(), String> {
    let real = std::env::var_os("SAMO_TEST_SETTINGS")
        .map(PathBuf::from)
        .ok_or("set SAMO_TEST_SETTINGS to a settings.json")?;
    let original = fs::read(&real).map_err(|e| format!("{}: {e}", real.display()))?;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    // Laid out as Samo lays it out, in a temporary home.
    let places = Places {
        home: dir.path().join("home"),
        data: dir
            .path()
            .join("Extensions")
            .join("claude-sessions")
            .join("data"),
    };
    fs::create_dir_all(places.home.join(".claude")).map_err(|e| e.to_string())?;
    fs::write(places.settings(), &original).map_err(|e| e.to_string())?;

    connection::connect(&places).map_err(|e| format!("connect: {e}"))?;
    assert!(connection::active(&places)?.connected);
    // Refused when a status line that isn't Samo's is set, as it should be; set otherwise.
    let lined = connection::plan_limits_on(&places);
    let had_other = connection::active(&places)?.other_statusline;
    assert!(lined.is_ok() || had_other, "{lined:?}");

    connection::plan_limits_off(&places)?;
    connection::disconnect(&places)?;
    let back = fs::read(places.settings()).map_err(|e| e.to_string())?;
    assert!(back == original, "settings.json changed");
    Ok(())
}
