//! The hook as Claude Code runs it (R9): silent, always exit 0, quick, whatever it is given.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn hook(data_dir: &std::path::Path, input: &[u8]) -> Result<Output, String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_samo-claude"))
        .arg("hook")
        .env("SAMO_EXT_DATA_DIR", data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        // A pipe may close early on huge input; that's the point.
        let _ = stdin.write_all(input);
    }
    child.wait_with_output().map_err(|e| e.to_string())
}

fn event(session: &str, name: &str) -> Vec<u8> {
    format!(
        r#"{{"session_id":"{session}","transcript_path":"/tmp/t.jsonl","cwd":"/tmp/proj","hook_event_name":"{name}"}}"#
    )
    .into_bytes()
}

#[test]
fn hook_is_silent_and_exits_zero_on_bad_input() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let data = dir.path().join("data");
    let huge = vec![b'x'; 2 * 1024 * 1024];
    let inputs: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"not json".to_vec(),
        b"[]".to_vec(),
        huge,
        event("../../evil", "UserPromptSubmit"),
        event("ok-1", "SomethingNew"),
        event("ok-1", "UserPromptSubmit"),
    ];
    for input in &inputs {
        let out = hook(&data, input)?;
        assert!(out.status.success(), "{:?}", out.status);
        assert!(
            out.stdout.is_empty(),
            "stdout: {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
    // Only the good event was recorded, and nothing escaped the data folder.
    let file = data.join("sessions/ok-1.json");
    let text = fs::read_to_string(&file).map_err(|e| e.to_string())?;
    assert!(text.contains("\"state\":\"working\""), "{text}");
    assert!(!dir.path().join("evil.json").exists());

    // A data folder it can't write to.
    let locked = dir.path().join("locked");
    fs::create_dir_all(&locked).map_err(|e| e.to_string())?;
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).map_err(|e| e.to_string())?;
    let out = hook(&locked.join("data"), &event("ok-2", "UserPromptSubmit"))?;
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o700));

    // The end of a session removes its file.
    let out = hook(&data, &event("ok-1", "SessionEnd"))?;
    assert!(out.status.success() && out.stdout.is_empty());
    assert!(!file.exists());
    Ok(())
}

#[test]
fn hook_exits_within_500_ms() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    // macOS checks a freshly built program the first time it runs; that isn't the hook's time.
    hook(dir.path(), b"")?;
    let started = Instant::now();
    // Claude Code never closes stdin.
    let mut child = Command::new(env!("CARGO_BIN_EXE_samo-claude"))
        .arg("hook")
        .env("SAMO_EXT_DATA_DIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let _stdin = child.stdin.take();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            assert!(status.success());
            break;
        }
        if started.elapsed() > Duration::from_secs(2) {
            let _ = child.kill();
            return Err("the hook was still running after 2 s".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let took = started.elapsed();
    assert!(took < Duration::from_millis(500), "took {took:?}");
    Ok(())
}
