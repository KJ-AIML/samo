//! Claude Code's settings (`~/.claude/settings.json`), changed only in ways Samo can undo exactly (ADR-002 R7).
//!
//! Pure: these functions take the parsed file and return the new one. Samo's own entries are recognised by
//! `OWN_MARK` in their command; everything else is left exactly as it was, in the same order. What Samo had to
//! create (the file, `hooks`, an event's list) is recorded, so undoing removes exactly that and nothing more.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::path::Path;

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

/// What Samo writes into Claude Code's settings on this OS: its hook entry, and its status line (`None` where no
/// command would run in Windows' shells).
#[derive(Debug, Clone, PartialEq)]
pub struct Entries {
    pub hook: Value,
    pub statusline: Option<Value>,
}

/// Samo's entries for the given OS (ADR-006 R25). `helper` is the helper program through `current`, and `home`
/// the user's home folder; the Mac's entries use neither and are unchanged.
///
/// On Windows Claude Code runs a hook's `command` with no shell when it has `args` (its exec form, CLI 2.1.139+).
/// The status line has no such form: it runs through Git Bash, or PowerShell without Git Bash. Both run an
/// unquoted path with forward slashes, so that is used; if it has a space (a user name with one), the path from
/// the home folder, `~/…`, which both shells expand.
pub fn entries_on(windows: bool, helper: &Path, home: &Path) -> Entries {
    if !windows {
        return Entries {
            hook: hook_entry(),
            statusline: Some(json!({ "type": "command", "command": helper_command("statusline") })),
        };
    }
    let exe = helper.to_string_lossy().into_owned();
    let hook = json!({
        "type": "command",
        "command": exe,
        "args": ["hook"],
        "async": true,
        "timeout": 5
    });
    let slashed = exe.replace('\\', "/");
    let home = home.to_string_lossy().replace('\\', "/");
    let home = home.trim_end_matches('/');
    let from_home = slashed
        .get(..home.len())
        .filter(|start| !home.is_empty() && start.eq_ignore_ascii_case(home))
        .and_then(|_| slashed.get(home.len()..))
        .and_then(|rest| rest.strip_prefix('/'))
        .map(|rest| format!("~/{rest}"));
    // Only what Git Bash and PowerShell both take as it is, unquoted: letters and digits (any script), and
    // `. _ - / : ~ +`. A space, `'`, `&`, `(`, `$`, `;` or a backtick, all legal in a profile path, would break one.
    let runnable = |path: &str| {
        path.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':' | '~' | '+'))
    };
    let path = if runnable(&slashed) {
        Some(slashed.clone())
    } else {
        from_home.filter(|p| runnable(p))
    };
    Entries {
        hook,
        statusline: path
            .map(|p| json!({ "type": "command", "command": format!("{p} statusline") })),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The file, or a part Samo must change, isn't the shape Claude Code documents.
    NotAnObject(&'static str),
    /// A status line that isn't Samo's is set; Samo never replaces it.
    OtherStatusLine,
    /// On Windows, the helper's path has a space and isn't under the home folder: no status line command would run in
    /// both of Windows' shells.
    StatusLinePath,
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
            Self::StatusLinePath => write!(
                f,
                "Samo is in a folder whose path has a space, and Windows' shells can't run a status line from \
                 there"
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

/// The helper as every Samo command runs it. On Windows a command is the helper's real path, which has no `OWN_MARK`
/// when `SAMO_EXT_ROOT` keeps extensions elsewhere.
const HELPER_MARK: &str = "/claude-sessions/current/bin/samo-claude";

/// Samo's own entry: its command runs Samo's helper. On Windows the hook's command is a path with backslashes.
fn is_ours(hook: &Value) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| {
            let c = c.replace('\\', "/");
            c.contains(OWN_MARK) || c.contains(HELPER_MARK)
        })
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
pub fn connect(
    mut v: Value,
    file_existed: bool,
    entries: &Entries,
) -> Result<(Value, Created), Refused> {
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
            groups.push(json!({ "hooks": [entries.hook.clone()] }));
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
pub fn set_statusline(mut v: Value, entries: &Entries) -> Result<Value, Refused> {
    match statusline_owner(&v) {
        Some(false) => Err(Refused::OtherStatusLine),
        Some(true) => Ok(v),
        None => {
            let line = entries.statusline.clone().ok_or(Refused::StatusLinePath)?;
            object_mut(&mut v, "The file")?.insert("statusLine".into(), line);
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

    /// The Mac's entries.
    fn mac() -> Entries {
        entries_on(false, Path::new("/x"), Path::new("/Users/a"))
    }

    #[test]
    fn connect_then_disconnect_is_equal_json() -> Result<(), String> {
        let win = windows(&helper_under(r"C:\Users\Jo"), r"C:\Users\Jo");
        for ((text, existed), entries) in [
            (USER, true),
            ("{}", true),
            (r#"{"model":"opus"}"#, true),
            (r#"{"hooks":{}}"#, true),
            (r#"{"hooks":{"Stop":[]}}"#, true),
            ("{}", false),
        ]
        .into_iter()
        .flat_map(|case| [(case, mac()), (case, win.clone())])
        {
            let original = parse(text);
            let (connected, created) =
                connect(original.clone(), existed, &entries).map_err(|e| e.to_string())?;
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
        let (connected, _) = connect(parse(USER), true, &mac()).map_err(|e| e.to_string())?;
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
        let (once, _) = connect(parse(USER), true, &mac()).map_err(|e| e.to_string())?;
        let (twice, created) = connect(once.clone(), true, &mac()).map_err(|e| e.to_string())?;
        assert_eq!(once, twice);
        assert_eq!(created, Created::default());
        assert!(is_connected(&twice));
        Ok(())
    }

    #[test]
    fn hook_entries_are_async_and_guarded() -> Result<(), String> {
        let (connected, _) = connect(parse("{}"), true, &mac()).map_err(|e| e.to_string())?;
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
            set_statusline(theirs.clone(), &mac()),
            Err(Refused::OtherStatusLine)
        );
        assert_eq!(clear_statusline(theirs.clone()), theirs);

        let ours =
            set_statusline(parse(r#"{"model":"opus"}"#), &mac()).map_err(|e| e.to_string())?;
        assert_eq!(statusline_owner(&ours), Some(true));
        assert_eq!(
            set_statusline(ours.clone(), &mac()).map_err(|e| e.to_string())?,
            ours
        );
        assert_eq!(clear_statusline(ours), parse(r#"{"model":"opus"}"#));
        Ok(())
    }

    #[test]
    fn refuses_settings_of_the_wrong_shape() {
        assert!(connect(parse("[]"), true, &mac()).is_err());
        assert!(connect(parse(r#"{"hooks":[]}"#), true, &mac()).is_err());
        assert!(connect(parse(r#"{"hooks":{"Stop":{}}}"#), true, &mac()).is_err());
        assert!(set_statusline(parse("[]"), &mac()).is_err());
    }

    /// Where Samo keeps the helper on Windows, for a user `home`.
    fn helper_under(home: &str) -> String {
        format!(r"{home}\AppData\Local\Samo\Extensions\claude-sessions\current\bin\samo-claude.exe")
    }

    fn windows(helper: &str, home: &str) -> Entries {
        entries_on(true, Path::new(helper), Path::new(home))
    }

    #[test]
    fn mac_entries_are_unchanged() {
        // The Mac's hook and status line, byte for byte (ADR-006 R25).
        let e = entries_on(false, Path::new("/anything"), Path::new("/Users/a"));
        assert_eq!(e.hook, hook_entry());
        assert_eq!(
            e.statusline,
            Some(json!({ "type": "command", "command": helper_command("statusline") }))
        );
    }

    #[test]
    fn windows_hooks_run_the_exe_without_a_shell() {
        let helper = helper_under(r"C:\Users\Jo");
        let e = windows(&helper, r"C:\Users\Jo");
        // Claude Code's exec form: no shell, so Git Bash or PowerShell make no difference (CLI 2.1.139+).
        assert_eq!(
            e.hook,
            json!({ "type": "command", "command": helper, "args": ["hook"], "async": true, "timeout": 5 })
        );
    }

    #[test]
    fn windows_statusline_runs_in_git_bash_and_powershell() {
        // Forward slashes, no quotes: the one form both of Windows' shells run as it is.
        let e = windows(&helper_under(r"C:\Users\Jo"), r"C:\Users\Jo");
        assert_eq!(
            e.statusline,
            Some(json!({
                "type": "command",
                "command": "C:/Users/Jo/AppData/Local/Samo/Extensions/claude-sessions/current/bin/samo-claude.exe statusline"
            }))
        );
        // A user name with a space: the path is given from the home folder, `~`, which both shells expand.
        let e = windows(&helper_under(r"C:\Users\Jo Smith"), r"C:\Users\Jo Smith");
        assert_eq!(
            e.statusline,
            Some(json!({
                "type": "command",
                "command": "~/AppData/Local/Samo/Extensions/claude-sessions/current/bin/samo-claude.exe statusline"
            }))
        );
    }

    #[test]
    fn windows_statusline_only_from_a_path_both_shells_run_as_it_is() {
        // `'`, `&`, `(`, `$` … are legal in a Windows profile path, and break the line in Git Bash or PowerShell.
        for user in ["O'Brien", "R&D", "Jo(1)", "Jo$x", "Jo`x", "Jo;x"] {
            let home = format!(r"C:\Users\{user}");
            let e = windows(&helper_under(&home), &home);
            assert_eq!(
                e.statusline,
                Some(json!({
                    "type": "command",
                    "command": "~/AppData/Local/Samo/Extensions/claude-sessions/current/bin/samo-claude.exe statusline"
                })),
                "{user}"
            );
        }
        // Outside the home folder, nothing both shells run: refused, with words.
        let e = windows(
            r"D:\R&D\Samo\Extensions\claude-sessions\current\bin\samo-claude.exe",
            r"C:\Users\Jo",
        );
        assert_eq!(e.statusline, None);
        // Letters beyond ASCII run in both.
        let e = windows(&helper_under(r"C:\Users\Jürgen"), r"C:\Users\Jürgen");
        assert_eq!(
            e.statusline,
            Some(json!({
                "type": "command",
                "command": "C:/Users/Jürgen/AppData/Local/Samo/Extensions/claude-sessions/current/bin/samo-claude.exe statusline"
            }))
        );
    }

    #[test]
    fn windows_statusline_refused_for_a_path_with_a_space() {
        // Outside the home folder, with a space: no form both shells run, so Samo says so and sets nothing.
        let helper = r"D:\Local Data\Samo\Extensions\claude-sessions\current\bin\samo-claude.exe";
        let e = windows(helper, r"C:\Users\Jo");
        assert_eq!(e.statusline, None);
        assert_eq!(
            set_statusline(parse("{}"), &e),
            Err(Refused::StatusLinePath)
        );
    }

    #[test]
    fn windows_entries_are_ours_wherever_extensions_are_kept() -> Result<(), String> {
        // SAMO_EXT_ROOT can put the Extensions folder anywhere; the hook's command is the helper's real path.
        let helper = r"D:\Work\Ext\claude-sessions\current\bin\samo-claude.exe";
        let e = windows(helper, r"C:\Users\Jo");
        let (connected, created) = connect(parse(USER), true, &e).map_err(|e| e.to_string())?;
        assert!(is_connected(&connected));
        let lined = set_statusline(connected, &e).map_err(|e| e.to_string())?;
        assert_eq!(statusline_owner(&lined), Some(true));
        let (back, _) = disconnect(clear_statusline(lined), Some(&created));
        assert_eq!(back, parse(USER));
        Ok(())
    }

    #[test]
    fn windows_entries_are_ours() -> Result<(), String> {
        let e = windows(&helper_under(r"C:\Users\Jo"), r"C:\Users\Jo");
        let (connected, created) = connect(parse(USER), true, &e).map_err(|e| e.to_string())?;
        for event in HOOK_EVENTS {
            assert_eq!(ours_in(&connected, event), 1, "{event}");
        }
        assert!(is_connected(&connected));
        let lined = set_statusline(connected, &e).map_err(|e| e.to_string())?;
        assert_eq!(statusline_owner(&lined), Some(true));
        let (back, _) = disconnect(clear_statusline(lined), Some(&created));
        assert_eq!(back, parse(USER));
        Ok(())
    }
}
