//! The status line as Claude Code runs it: one short line, exit 0, and the limits saved for Samo.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn statusline(data_dir: &std::path::Path, input: &[u8]) -> Result<Output, String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_samo-claude"))
        .arg("statusline")
        .env("SAMO_EXT_DATA_DIR", data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    child.wait_with_output().map_err(|e| e.to_string())
}

#[test]
fn statusline_prints_one_line_and_exits_zero_on_garbage() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    for garbage in [&b""[..], b"nope", b"[1,2]", b"{}"] {
        let out = statusline(dir.path(), garbage)?;
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).lines().count() <= 1);
    }
    assert!(!dir.path().join("limits.json").exists());

    let soon = chrono::Utc::now().timestamp() + 7_200;
    let input = format!(
        r#"{{"session_id":"s1","model":{{"id":"claude-opus-5-5","display_name":"Opus"}},"rate_limits":{{"five_hour":{{"used_percentage":34,"resets_at":{soon}}},"seven_day":{{"used_percentage":12,"resets_at":{}}}}}}}"#,
        soon + 86_400
    );
    let out = statusline(dir.path(), input.as_bytes())?;
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "5h 34% · wk 12%\n");
    let saved =
        std::fs::read_to_string(dir.path().join("limits.json")).map_err(|e| e.to_string())?;
    assert!(saved.contains("\"used_percentage\":34"), "{saved}");
    Ok(())
}
