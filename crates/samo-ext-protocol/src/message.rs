use crate::{ProtocolError, Widget};
use samo_primitives::Reading;
use serde::{Deserialize, Serialize};

/// Samo to a helper, one JSON object per line on the helper's stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostMsg {
    Hello {
        protocol: u32,
        samo_version: String,
        data_dir: String,
    },
    /// The user flipped a switch. The helper acts, then reports the result in its next `view`; it may refuse.
    Setting { key: String, value: bool },
    /// The user pressed a button.
    Action { id: String },
    /// Whether the extension's page is on screen, so the helper can skip pop-ups the user would see anyway.
    Visible { page: bool },
    /// Samo is stopping the helper; it should exit within 2 s.
    Shutdown,
}

/// A helper to Samo, one JSON object per line on the helper's stdout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HelperMsg {
    /// The first thing a helper says, within 5 s.
    Hello { protocol: u32 },
    /// The whole page; it replaces the last one.
    View { sections: Vec<Widget> },
    /// How many things wait for the user (shown beside Samo's menu bar figure); `0` clears it. `summary` is the
    /// quick panel's line, and `meter`, if any, a small load bar at its end.
    Badge {
        count: u32,
        #[serde(default)]
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meter: Option<BadgeMeter>,
    },
    /// A short note under the menu bar item. `key` names what it is about.
    Popup {
        key: String,
        kind: PopupKind,
        title: String,
        #[serde(default)]
        body: String,
    },
    /// Something worth a line in Samo's `history.log` (e.g. it changed another app's settings).
    Log {
        event: String,
        #[serde(default)]
        detail: String,
    },
    Health {
        ok: bool,
        #[serde(default)]
        message: String,
    },
}

/// A small load bar on an extension's quick-panel line: a short label and a share, `0..=1`. Unknown lights
/// nothing and reads `--`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BadgeMeter {
    pub label: String,
    pub fraction: Reading<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupKind {
    NeedsYou,
    Finished,
    Info,
}

pub fn decode_helper_line(line: &str) -> Result<HelperMsg, ProtocolError> {
    serde_json::from_str(line.trim()).map_err(|e| ProtocolError::Message(e.to_string()))
}

pub fn decode_host_line(line: &str) -> Result<HostMsg, ProtocolError> {
    serde_json::from_str(line.trim()).map_err(|e| ProtocolError::Message(e.to_string()))
}

/// One message as one line, without the newline. Compact JSON never contains a raw newline.
pub fn encode<T: Serialize>(msg: &T) -> Result<String, ProtocolError> {
    serde_json::to_string(msg).map_err(|e| ProtocolError::Message(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ButtonKind, Lamp, NoteKind, RowItem, rows_in};
    use samo_primitives::{Reading, UnsupportedReason};

    fn every_widget() -> Vec<Widget> {
        vec![
            Widget::Section {
                title: "SESSIONS · LIVE".into(),
                about: Some("2 running".into()),
            },
            Widget::Figure {
                label: "Tokens today".into(),
                value: Reading::available(1_234_567.0, 1),
                unit: "tokens".into(),
                note: None,
            },
            Widget::Figure {
                label: "Unknown".into(),
                value: Reading::unsupported(UnsupportedReason::SourceNotFound),
                unit: String::new(),
                note: Some("Claude Code's transcripts weren't found".into()),
            },
            Widget::Meter {
                label: "5-hour".into(),
                fraction: Reading::stopped(5),
                note: Some("reset at 18:40".into()),
            },
            Widget::Rows {
                items: vec![RowItem {
                    lamp: Lamp::NeedsYou,
                    title: "samo".into(),
                    detail: "permission: Bash".into(),
                    trailing: "12.3k".into(),
                    tag: Some("dev".into()),
                }],
            },
            Widget::Switch {
                key: "connect".into(),
                label: "Connect to Claude Code".into(),
                on: true,
                note: None,
            },
            Widget::Button {
                action: "ping".into(),
                label: "Ping".into(),
                kind: ButtonKind::Link,
            },
            Widget::Note {
                text: "Something went wrong".into(),
                kind: NoteKind::Error,
            },
        ]
    }

    #[test]
    fn every_message_round_trips() -> Result<(), String> {
        let host = vec![
            HostMsg::Hello {
                protocol: 1,
                samo_version: "0.1.0".into(),
                data_dir: "/tmp/x y/data".into(),
            },
            HostMsg::Setting {
                key: "connect".into(),
                value: true,
            },
            HostMsg::Action { id: "ping".into() },
            HostMsg::Visible { page: false },
            HostMsg::Shutdown,
        ];
        for msg in host {
            let line = encode(&msg).map_err(|e| e.to_string())?;
            assert!(!line.contains('\n'));
            assert_eq!(decode_host_line(&line).map_err(|e| e.to_string())?, msg);
        }

        let helper = vec![
            HelperMsg::Hello { protocol: 1 },
            HelperMsg::View {
                sections: every_widget(),
            },
            HelperMsg::Badge {
                count: 2,
                summary: "Claude · 2 need you".into(),
                meter: None,
            },
            HelperMsg::Badge {
                count: 0,
                summary: "Claude · 1 working".into(),
                meter: Some(BadgeMeter {
                    label: "5h".into(),
                    fraction: Reading::available(0.36, 9),
                }),
            },
            HelperMsg::Popup {
                key: "s1".into(),
                kind: PopupKind::NeedsYou,
                title: "samo needs you".into(),
                body: "permission: Bash\nsecond line".into(),
            },
            HelperMsg::Log {
                event: "connect".into(),
                detail: "~/.claude/settings.json".into(),
            },
            HelperMsg::Health {
                ok: false,
                message: "running in another copy of Samo".into(),
            },
        ];
        for msg in helper {
            let line = encode(&msg).map_err(|e| e.to_string())?;
            assert!(!line.contains('\n'), "{line}");
            assert_eq!(decode_helper_line(&line).map_err(|e| e.to_string())?, msg);
        }
        Ok(())
    }

    #[test]
    fn unknown_type_is_error_not_panic() {
        for line in [
            r#"{"type":"explode"}"#,
            r#"{"count":1}"#,
            "not json",
            "",
            "[]",
            r#"{"type":"badge","count":-1}"#,
            r#"{"type":"view","sections":[{"widget":"canvas"}]}"#,
        ] {
            assert!(decode_helper_line(line).is_err(), "{line}");
        }
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let line = r#"{"type":"badge","count":2,"summary":"x","colour":"red"}"#;
        assert_eq!(
            decode_helper_line(line).ok(),
            Some(HelperMsg::Badge {
                count: 2,
                summary: "x".into(),
                meter: None
            })
        );
        // Optional fields may be left out.
        assert_eq!(
            decode_helper_line(r#"{"type":"badge","count":0}"#).ok(),
            Some(HelperMsg::Badge {
                count: 0,
                summary: String::new(),
                meter: None
            })
        );
    }

    #[test]
    fn no_primary_or_danger_button_kind() {
        for kind in ["primary", "danger"] {
            let line = format!(
                r#"{{"type":"view","sections":[{{"widget":"button","action":"a","label":"b","kind":"{kind}"}}]}}"#
            );
            assert!(decode_helper_line(&line).is_err(), "{kind}");
        }
        let fine = r#"{"type":"view","sections":[{"widget":"button","action":"a","label":"b"}]}"#;
        assert!(decode_helper_line(fine).is_ok());
    }

    #[test]
    fn rows_are_counted() {
        assert_eq!(rows_in(&every_widget()), 8);
        assert_eq!(rows_in(&[Widget::Rows { items: vec![] }]), 1);
    }
}
