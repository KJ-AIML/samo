//! `serve` as Samo runs it.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn second_serve_reports_other_copy() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let spawn = || {
        Command::new(env!("CARGO_BIN_EXE_samo-claude"))
            .arg("serve")
            .env("HOME", dir.path())
            .env("SAMO_EXT_DATA_DIR", dir.path().join("data"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())
    };
    let mut first = spawn()?;
    let first_out = first.stdout.take().ok_or("no stdout")?;
    let mut first_lines = BufReader::new(first_out).lines();
    // It says hello, then sends its page.
    let hello = first_lines
        .next()
        .ok_or("no hello")?
        .map_err(|e| e.to_string())?;
    assert!(hello.contains("\"hello\""), "{hello}");
    let page = first_lines
        .next()
        .ok_or("no page")?
        .map_err(|e| e.to_string())?;
    assert!(page.contains("SESSIONS · LIVE"), "{page}");

    let second = spawn()?;
    let out = second.wait_with_output().map_err(|e| e.to_string())?;
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(
        lines[1].contains("\"ok\":false") && lines[1].contains("another copy of Samo"),
        "{text}"
    );

    // Shutdown ends the first.
    if let Some(mut stdin) = first.stdin.take() {
        writeln!(stdin, r#"{{"type":"shutdown"}}"#).map_err(|e| e.to_string())?;
    }
    for _ in 0..100 {
        if first.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = first.kill();
    Err("serve didn't exit after shutdown".into())
}
