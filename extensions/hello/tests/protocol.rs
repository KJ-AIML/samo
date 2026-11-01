//! Hello speaks valid protocol v1 from its first line to its last.

use samo_ext_protocol::{HelperMsg, PopupKind, Widget, decode_helper_line};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn hello_speaks_valid_protocol_v1() -> Result<(), String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_samo-ext-hello"))
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    {
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        for line in [
            r#"{"type":"hello","protocol":1,"samo_version":"0.1.0","data_dir":"/tmp"}"#,
            r#"{"type":"setting","key":"greet","value":false}"#,
            r#"{"type":"action","id":"ping"}"#,
            r#"{"type":"visible","page":true}"#,
            r#"{"type":"shutdown"}"#,
        ] {
            writeln!(stdin, "{line}").map_err(|e| e.to_string())?;
        }
    }
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut messages = Vec::new();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| e.to_string())?;
        messages.push(decode_helper_line(&line).map_err(|e| format!("{e}: {line}"))?);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    assert!(status.success());

    assert_eq!(messages.first(), Some(&HelperMsg::Hello { protocol: 1 }));
    let Some(HelperMsg::View { sections }) = messages
        .iter()
        .find(|m| matches!(m, HelperMsg::View { .. }))
    else {
        return Err("no view".into());
    };
    let kinds: Vec<&str> = sections
        .iter()
        .map(|w| match w {
            Widget::Section { .. } => "section",
            Widget::Figure { .. } => "figure",
            Widget::Meter { .. } => "meter",
            Widget::Rows { .. } => "rows",
            Widget::Switch { .. } => "switch",
            Widget::Button { .. } => "button",
            Widget::Note { .. } => "note",
        })
        .collect();
    for kind in [
        "section", "figure", "meter", "rows", "switch", "button", "note",
    ] {
        assert!(kinds.contains(&kind), "no {kind} in {kinds:?}");
    }
    assert!(messages.contains(&HelperMsg::Badge {
        count: 1,
        summary: "Hello · 1 waiting".into()
    }));
    // Turning the switch off clears the badge, and Ping pops up.
    assert!(messages.contains(&HelperMsg::Badge {
        count: 0,
        summary: String::new()
    }));
    assert!(messages.iter().any(|m| matches!(
        m,
        HelperMsg::Popup {
            kind: PopupKind::Info,
            ..
        }
    )));
    Ok(())
}
