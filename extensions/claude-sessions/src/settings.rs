//! Claude Code's settings (`~/.claude/settings.json`), changed only in ways Samo can undo exactly (ADR-002 R7).
//!
//! Pure: these functions take the parsed file and return the new one. Samo's own entries are recognised by
//! `OWN_MARK` in their command; everything else is left exactly as it was, in the same order. What Samo had to
//! create (the file, `hooks`, an event's list) is recorded, so undoing removes exactly that and nothing more.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Samo's hooks and status line are recognised by this in their command.
pub const OWN_MARK: &str = "/Samo/Extensions/claude-sessions/";

/// The hook events Claude Sessions listens to.
pub const HOOK_EVENTS: [&str; 10] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// The shell command Claude Code runs. If the extension has been removed, it does nothing and still succeeds.
pub fn helper_command(verb: &str) -> String {
    format!(
        "H=\"$HOME/Library/Application Support/Samo/Extensions/claude-sessions/current/bin/samo-claude\"; \
         [ -x \"$H\" ] && \"$H\" {verb}; exit 0"
    )
}

/// One hook entry: run async (never holding Claude Code up), with no matcher (every tool).
pub fn hook_entry() -> Value {
    json!({
        "type": "command",
        "command": helper_command("hook"),
        "async": true,
        "timeout": 5
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The file, or a part Samo must change, isn't the shape Claude Code documents.
    NotAnObject(&'static str),
    /// A status line that isn't Samo's is set; Samo never replaces it.
    OtherStatusLine,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnObject(what) => {
                write!(f, "{what} in settings.json isn't what Claude Code expects")
            }
            Self::OtherStatusLine => write!(
                f,
                "you already use a status line, and Samo never replaces it"
            ),
        }
    }
}

/// What Samo created while connecting, so disconnecting removes exactly that.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Created {
    /// The settings file itself didn't exist.
    #[serde(default)]
    pub file: bool,
    /// `hooks` didn't exist.
    #[serde(default)]
    pub hooks: bool,
    /// Events whose list didn't exist.
    #[serde(default)]
    pub events: Vec<String>,
}

fn is_ours(hook: &Value) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains(OWN_MARK))
}

fn group_has_ours(group: &Value) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| hooks.iter().any(is_ours))
}

fn object_mut<'a>(
    v: &'a mut Value,
    what: &'static str,
) -> Result<&'a mut Map<String, Value>, Refused> {
    v.as_object_mut().ok_or(Refused::NotAnObject(what))
}

/// Whether Samo's hooks are in the settings.
pub fn is_connected(v: &Value) -> bool {
    v.get("hooks")
        .and_then(Value::as_object)
        .is_some_and(|hooks| {
            hooks
                .values()
                .filter_map(Value::as_array)
                .any(|groups| groups.iter().any(group_has_ours))
        })
}

/// Adds Samo's hook to each event in `HOOK_EVENTS`, once. `file_existed` says whether the file was there.
pub fn connect(mut v: Value, file_existed: bool) -> Result<(Value, Created), Refused> {
    let mut created = Created {
        file: !file_existed,
        ..Created::default()
    };
    let root = object_mut(&mut v, "The file")?;
    if !root.contains_key("hooks") {
        root.insert("hooks".into(), Value::Object(Map::new()));
        created.hooks = true;
    }
    let hooks = root
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .ok_or(Refused::NotAnObject("`hooks`"))?;
    for event in HOOK_EVENTS {
        if !hooks.contains_key(event) {
            hooks.insert(event.into(), Value::Array(Vec::new()));
            created.events.push(event.to_string());
        }
        let groups = hooks
            .get_mut(event)
            .and_then(Value::as_array_mut)
            .ok_or(Refused::NotAnObject("A hook event"))?;
        if !groups.iter().any(group_has_ours) {
            groups.push(json!({ "hooks": [hook_entry()] }));
        }
    }
    Ok((v, created))
}

/// Removes Samo's hooks, and whatever Samo created to hold them. Without a record of what was created, a
/// container left empty by removing Samo's entries is removed. Returns whether the file should be deleted
/// (Samo created it, and nothing else is in it now).
pub fn disconnect(mut v: Value, created: Option<&Created>) -> (Value, bool) {
    let Some(root) = v.as_object_mut() else {
        return (v, false);
    };
    let mut drop_hooks = false;
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        let events: Vec<String> = hooks.keys().cloned().collect();
        for event in events {
            let Some(groups) = hooks.get_mut(&event).and_then(Value::as_array_mut) else {
                continue;
            };
            let before = groups.len();
            let mut removed_any = false;
            for group in groups.iter_mut() {
                if let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                    let n = list.len();
                    list.retain(|h| !is_ours(h));
                    removed_any |= list.len() != n;
                }
            }
            // A group that held only Samo's hook goes too.
            groups.retain(|g| {
                g.get("hooks")
                    .and_then(Value::as_array)
                    .is_none_or(|list| !list.is_empty())
            });
            let emptied = groups.is_empty() && (removed_any || groups.len() != before);
            let samo_made_it = match created {
                Some(c) => c.events.iter().any(|e| e == &event),
                None => emptied,
            };
            if groups.is_empty() && samo_made_it {
                hooks.shift_remove(&event);
            }
        }
        drop_hooks = hooks.is_empty()
            && match created {
                Some(c) => c.hooks,
                None => true,
            };
    }
    if drop_hooks {
        root.shift_remove("hooks");
    }
    let delete_file = created.is_some_and(|c| c.file) && root.is_empty();
    (v, delete_file)
}

/// Whose status line is set: `None` if there is none.
pub fn statusline_owner(v: &Value) -> Option<bool> {
    let line = v.get("statusLine")?;
    Some(is_ours(line))
}

/// Sets Samo's status line, unless one that isn't Samo's is set.
pub fn set_statusline(mut v: Value) -> Result<Value, Refused> {
    match statusline_owner(&v) {
        Some(false) => Err(Refused::OtherStatusLine),
        Some(true) => Ok(v),
        None => {
            object_mut(&mut v, "The file")?.insert(
                "statusLine".into(),
                json!({ "type": "command", "command": helper_command("statusline") }),
            );
            Ok(v)
        }
    }
}

/// Removes the status line, only if it is Samo's.
pub fn clear_statusline(mut v: Value) -> Value {
    if statusline_owner(&v) == Some(true)
        && let Some(root) = v.as_object_mut()
    {
        root.shift_remove("statusLine");
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).unwrap_or(Value::Null)
    }

    fn ours_in(v: &Value, event: &str) -> usize {
        v["hooks"][event]
            .as_array()
            .map(|groups| groups.iter().filter(|g| group_has_ours(g)).count())
            .unwrap_or(0)
    }

    const USER: &str = r#"{
        "model": "opus",
        "hooks": {
            "Stop": [ { "hooks": [ { "type": "command", "command": "say done" } ] } ],
            "PreToolUse": [ { "matcher": "Bash", "hooks": [ { "type": "command", "command": "./guard.sh" } ] } ],
            "Custom": []
        },
        "permissions": { "allow": ["Bash(ls:*)"] }
    }"#;

    #[test]
    fn connect_then_disconnect_is_equal_json() -> Result<(), String> {
        for (text, existed) in [
            (USER, true),
            ("{}", true),
            (r#"{"model":"opus"}"#, true),
            (r#"{"hooks":{}}"#, true),
            (r#"{"hooks":{"Stop":[]}}"#, true),
            ("{}", false),
        ] {
            let original = parse(text);
            let (connected, created) =
                connect(original.clone(), existed).map_err(|e| e.to_string())?;
            for event in HOOK_EVENTS {
                assert_eq!(ours_in(&connected, event), 1, "{event} in {text}");
            }
            let (back, delete) = disconnect(connected, Some(&created));
            assert_eq!(back, original, "{text}");
            // Key order is kept too.
            assert_eq!(
                serde_json::to_string(&back).map_err(|e| e.to_string())?,
                serde_json::to_string(&original).map_err(|e| e.to_string())?
            );
            assert_eq!(delete, !existed);
        }
        Ok(())
    }

    #[test]
    fn disconnect_keeps_user_hooks() -> Result<(), String> {
        let (connected, _) = connect(parse(USER), true).map_err(|e| e.to_string())?;
        // Without the record (say the data folder was lost), the user's things still survive.
        let (back, _) = disconnect(connected, None);
        assert_eq!(back["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
        assert_eq!(back["hooks"]["PreToolUse"][0]["matcher"], "Bash");
        assert!(
            back["hooks"]["Custom"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );
        assert!(!is_connected(&back));
        assert_eq!(back["permissions"]["allow"][0], "Bash(ls:*)");
        Ok(())
    }

    #[test]
    fn connect_twice_adds_once() -> Result<(), String> {
        let (once, _) = connect(parse(USER), true).map_err(|e| e.to_string())?;
        let (twice, created) = connect(once.clone(), true).map_err(|e| e.to_string())?;
        assert_eq!(once, twice);
        assert_eq!(created, Created::default());
        assert!(is_connected(&twice));
        Ok(())
    }

    #[test]
    fn hook_entries_are_async_and_guarded() -> Result<(), String> {
        let (connected, _) = connect(parse("{}"), true).map_err(|e| e.to_string())?;
        for event in HOOK_EVENTS {
            let entry = &connected["hooks"][event][0]["hooks"][0];
            assert_eq!(entry["async"], true);
            assert!(connected["hooks"][event][0].get("matcher").is_none());
            let command = entry["command"].as_str().unwrap_or_default();
            assert!(command.contains(OWN_MARK));
            assert!(command.contains("[ -x \"$H\" ]") && command.ends_with("exit 0"));
        }
        Ok(())
    }

    #[test]
    fn never_replaces_statusline() -> Result<(), String> {
        let theirs = parse(r#"{"statusLine":{"type":"command","command":"~/bin/line.sh"}}"#);
        assert_eq!(
            set_statusline(theirs.clone()),
            Err(Refused::OtherStatusLine)
        );
        assert_eq!(clear_statusline(theirs.clone()), theirs);

        let ours = set_statusline(parse(r#"{"model":"opus"}"#)).map_err(|e| e.to_string())?;
        assert_eq!(statusline_owner(&ours), Some(true));
        assert_eq!(
            set_statusline(ours.clone()).map_err(|e| e.to_string())?,
            ours
        );
        assert_eq!(clear_statusline(ours), parse(r#"{"model":"opus"}"#));
        Ok(())
    }

    #[test]
    fn refuses_settings_of_the_wrong_shape() {
        assert!(connect(parse("[]"), true).is_err());
        assert!(connect(parse(r#"{"hooks":[]}"#), true).is_err());
        assert!(connect(parse(r#"{"hooks":{"Stop":{}}}"#), true).is_err());
        assert!(set_statusline(parse("[]")).is_err());
    }
}
