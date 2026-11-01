//! What state each Claude Code session is in, from its hook events (ADR-002 C1), and what Samo says about it
//! (R10: something true). Pure: the time and whether Claude's process is alive are passed in.

use samo_primitives::ProcessIdentity;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// After this long with no hook event and no transcript write, a working session says so.
pub const NO_NEWS_MS: i64 = 5 * 60 * 1_000;
/// A session whose Claude process Samo can't check is dropped after this long without an event.
pub const STALE_MS: i64 = 12 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Working,
    NeedsYou,
    Done,
}

/// One session, as `data/sessions/<session_id>.json` keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionFile {
    pub session_id: String,
    pub state: State,
    /// Why it waits or how it ended: `permission: Bash`, `a question`, `ready`, `interrupted` …
    #[serde(default)]
    pub reason: String,
    /// It stopped on an error (Claude Code's `StopFailure`).
    #[serde(default)]
    pub error: bool,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub transcript_path: String,
    /// When the last event was applied.
    pub updated_at_ms: i64,
    /// When the state last changed.
    pub since_ms: i64,
    /// When the current turn began (the prompt), for "finished after 15 s".
    #[serde(default)]
    pub turn_started_ms: Option<i64>,
    #[serde(default)]
    pub background_tasks: u32,
    /// Claude Code's process, so a session whose terminal closed without `SessionEnd` is noticed (R4's identity).
    #[serde(default)]
    pub claude: Option<ProcessIdentity>,
}

/// The parts of Claude Code's hook input Samo reads (documented at code.claude.com/docs/en/hooks).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HookEvent {
    pub session_id: String,
    pub hook_event_name: String,
    #[serde(default)]
    pub transcript_path: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub is_interrupt: Option<bool>,
    #[serde(default)]
    pub background_tasks: Option<Vec<Value>>,
}

/// A session id that is safe as a file name.
pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Apply {
    Write(SessionFile),
    Remove,
    Keep,
}

/// What an event does to a session. Events older than the file are ignored: async hooks can finish out of order.
pub fn apply(prev: Option<&SessionFile>, ev: &HookEvent, now_ms: i64) -> Apply {
    if prev.is_some_and(|p| now_ms < p.updated_at_ms) {
        return Apply::Keep;
    }
    // A turn runs from the prompt to done, through any wait for you.
    let in_turn = prev.is_some_and(|p| p.state != State::Done);
    let turn_started = || {
        if in_turn {
            prev.and_then(|p| p.turn_started_ms).or(Some(now_ms))
        } else {
            Some(now_ms)
        }
    };
    let (state, reason, error, turn) = match ev.hook_event_name.as_str() {
        // A resumed or compacted session already has a state; keep it.
        "SessionStart" if prev.is_some() => return Apply::Keep,
        "SessionStart" => (State::Done, "ready".to_string(), false, None),
        "UserPromptSubmit" => (State::Working, String::new(), false, Some(now_ms)),
        "PreToolUse" => match ev.tool_name.as_deref() {
            Some("AskUserQuestion") => {
                (State::NeedsYou, "a question".into(), false, turn_started())
            }
            Some("ExitPlanMode") => (
                State::NeedsYou,
                "a plan to review".into(),
                false,
                turn_started(),
            ),
            _ => (State::Working, String::new(), false, turn_started()),
        },
        "PostToolUse" => (State::Working, String::new(), false, turn_started()),
        "PostToolUseFailure" if ev.is_interrupt == Some(true) => {
            (State::Done, "interrupted".into(), false, None)
        }
        "PostToolUseFailure" => (State::Working, String::new(), false, turn_started()),
        // Claude Code asks permission for its own question and plan tools too; say what they really are.
        "PermissionRequest" => match ev.tool_name.as_deref() {
            Some("AskUserQuestion") => {
                (State::NeedsYou, "a question".into(), false, turn_started())
            }
            Some("ExitPlanMode") => (
                State::NeedsYou,
                "a plan to review".into(),
                false,
                turn_started(),
            ),
            tool => {
                let tool = tool.unwrap_or("a tool");
                (
                    State::NeedsYou,
                    format!("permission: {tool}"),
                    false,
                    turn_started(),
                )
            }
        },
        "Notification" => match ev.notification_type.as_deref() {
            // A backstop for PermissionRequest; keep the more exact reason if it already came.
            Some("permission_prompt")
                if prev.is_some_and(|p| {
                    p.state == State::NeedsYou && p.reason.starts_with("permission")
                }) =>
            {
                return Apply::Keep;
            }
            Some("permission_prompt") => {
                (State::NeedsYou, "permission".into(), false, turn_started())
            }
            Some("elicitation_dialog" | "elicitation_url_dialog" | "agent_needs_input") => (
                State::NeedsYou,
                "input needed".into(),
                false,
                turn_started(),
            ),
            // `idle_prompt` is a timing guess, not a question; the rest say nothing about the state.
            _ => return Apply::Keep,
        },
        "Stop" => (State::Done, "done".into(), false, None),
        "StopFailure" => (State::Done, "stopped: API error".into(), true, None),
        "SessionEnd" => return Apply::Remove,
        _ => return Apply::Keep,
    };
    let keep = |new: &str, old: Option<&String>| {
        if new.is_empty() {
            old.cloned().unwrap_or_default()
        } else {
            new.to_string()
        }
    };
    let since_ms = match prev {
        Some(p) if p.state == state && p.reason == reason && p.error == error => p.since_ms,
        _ => now_ms,
    };
    Apply::Write(SessionFile {
        session_id: ev.session_id.clone(),
        state,
        reason,
        error,
        cwd: keep(&ev.cwd, prev.map(|p| &p.cwd)),
        transcript_path: keep(&ev.transcript_path, prev.map(|p| &p.transcript_path)),
        updated_at_ms: now_ms,
        since_ms,
        turn_started_ms: turn,
        background_tasks: if ev.hook_event_name == "Stop" {
            ev.background_tasks
                .as_ref()
                .map_or(0, |t| u32::try_from(t.len()).unwrap_or(u32::MAX))
        } else {
            prev.map_or(0, |p| p.background_tasks)
        },
        claude: prev.and_then(|p| p.claude),
    })
}

/// One session as Samo shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub session_id: String,
    /// The folder the session runs in.
    pub project: String,
    pub state: State,
    pub error: bool,
    pub detail: String,
}

/// `12 s`, `4 min`, `2 h 5 min`.
pub fn duration(ms: i64) -> String {
    let secs = ms.max(0) / 1_000;
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3_599 => format!("{} min", secs / 60),
        _ => match (secs / 3_600, (secs % 3_600) / 60) {
            (h, 0) => format!("{h} h"),
            (h, m) => format!("{h} h {m} min"),
        },
    }
}

fn project_name(cwd: &str) -> String {
    cwd.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("a session")
        .to_string()
}

/// What Samo says about a session, or `None` if it's gone: its Claude process ended (`alive` is `Some(false)`), or
/// without a way to check, nothing has happened for 12 hours. `last_activity_ms` is the newest of its last event
/// and its transcript's last write.
pub fn derive(
    file: &SessionFile,
    now_ms: i64,
    alive: Option<bool>,
    last_activity_ms: i64,
) -> Option<Row> {
    if alive == Some(false) {
        return None;
    }
    if alive.is_none() && now_ms - file.updated_at_ms > STALE_MS {
        return None;
    }
    let quiet = now_ms - last_activity_ms.max(file.updated_at_ms);
    let detail = match file.state {
        State::Working if quiet > NO_NEWS_MS => {
            format!("working · no news for {}", duration(quiet))
        }
        State::Working => format!(
            "working · {}",
            duration(now_ms - file.turn_started_ms.unwrap_or(file.since_ms))
        ),
        State::NeedsYou => format!("{} · {}", file.reason, duration(now_ms - file.since_ms)),
        State::Done if file.background_tasks > 0 => format!(
            "{} · {} background task{} · {} ago",
            file.reason,
            file.background_tasks,
            if file.background_tasks == 1 { "" } else { "s" },
            duration(now_ms - file.since_ms)
        ),
        State::Done => format!("{} · {} ago", file.reason, duration(now_ms - file.since_ms)),
    };
    Some(Row {
        session_id: file.session_id.clone(),
        project: project_name(&file.cwd),
        state: file.state,
        error: file.error,
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> HookEvent {
        let text = match name {
            "session_start" => include_str!("../tests/fixtures/hooks/session_start.json"),
            "user_prompt_submit" => include_str!("../tests/fixtures/hooks/user_prompt_submit.json"),
            "pre_tool_use_bash" => include_str!("../tests/fixtures/hooks/pre_tool_use_bash.json"),
            "pre_tool_use_ask" => include_str!("../tests/fixtures/hooks/pre_tool_use_ask.json"),
            "pre_tool_use_exit_plan" => {
                include_str!("../tests/fixtures/hooks/pre_tool_use_exit_plan.json")
            }
            "post_tool_use" => include_str!("../tests/fixtures/hooks/post_tool_use.json"),
            "post_tool_use_failure" => {
                include_str!("../tests/fixtures/hooks/post_tool_use_failure.json")
            }
            "post_tool_use_failure_interrupt" => {
                include_str!("../tests/fixtures/hooks/post_tool_use_failure_interrupt.json")
            }
            "permission_request" => include_str!("../tests/fixtures/hooks/permission_request.json"),
            "notification_permission_prompt" => {
                include_str!("../tests/fixtures/hooks/notification_permission_prompt.json")
            }
            "notification_elicitation" => {
                include_str!("../tests/fixtures/hooks/notification_elicitation.json")
            }
            "notification_idle" => include_str!("../tests/fixtures/hooks/notification_idle.json"),
            "stop" => include_str!("../tests/fixtures/hooks/stop.json"),
            "stop_background" => include_str!("../tests/fixtures/hooks/stop_background.json"),
            "stop_failure" => include_str!("../tests/fixtures/hooks/stop_failure.json"),
            "session_end" => include_str!("../tests/fixtures/hooks/session_end.json"),
            _ => "{}",
        };
        serde_json::from_str(text).unwrap_or_default()
    }

    fn working(at: i64) -> SessionFile {
        SessionFile {
            session_id: "SESSION".into(),
            state: State::Working,
            reason: String::new(),
            error: false,
            cwd: "/Users/kj/Developer/samo".into(),
            transcript_path: "TRANSCRIPT".into(),
            updated_at_ms: at,
            since_ms: at,
            turn_started_ms: Some(at),
            background_tasks: 0,
            claude: None,
        }
    }

    fn state_after(prev: Option<&SessionFile>, name: &str) -> Option<(State, String, bool)> {
        match apply(prev, &fixture(name), 10_000) {
            Apply::Write(f) => Some((f.state, f.reason, f.error)),
            Apply::Remove | Apply::Keep => None,
        }
    }

    #[test]
    fn each_documented_event_maps_per_table() {
        let w = working(1_000);
        let s = |state, reason: &str| Some((state, reason.to_string(), false));
        assert_eq!(state_after(None, "session_start"), s(State::Done, "ready"));
        assert_eq!(
            apply(Some(&w), &fixture("session_start"), 10_000),
            Apply::Keep
        );
        assert_eq!(
            state_after(None, "user_prompt_submit"),
            s(State::Working, "")
        );
        assert_eq!(
            state_after(Some(&w), "pre_tool_use_bash"),
            s(State::Working, "")
        );
        assert_eq!(
            state_after(Some(&w), "pre_tool_use_ask"),
            s(State::NeedsYou, "a question")
        );
        assert_eq!(
            state_after(Some(&w), "pre_tool_use_exit_plan"),
            s(State::NeedsYou, "a plan to review")
        );
        assert_eq!(
            state_after(Some(&w), "permission_request"),
            s(State::NeedsYou, "permission: Bash")
        );
        assert_eq!(
            state_after(Some(&w), "notification_permission_prompt"),
            s(State::NeedsYou, "permission")
        );
        assert_eq!(
            state_after(Some(&w), "notification_elicitation"),
            s(State::NeedsYou, "input needed")
        );
        assert_eq!(
            apply(Some(&w), &fixture("notification_idle"), 10_000),
            Apply::Keep
        );
        assert_eq!(
            state_after(Some(&w), "post_tool_use"),
            s(State::Working, "")
        );
        assert_eq!(
            state_after(Some(&w), "post_tool_use_failure"),
            s(State::Working, "")
        );
        assert_eq!(
            state_after(Some(&w), "post_tool_use_failure_interrupt"),
            s(State::Done, "interrupted")
        );
        assert_eq!(state_after(Some(&w), "stop"), s(State::Done, "done"));
        assert_eq!(
            state_after(Some(&w), "stop_failure"),
            Some((State::Done, "stopped: API error".into(), true))
        );
        assert_eq!(
            apply(Some(&w), &fixture("session_end"), 10_000),
            Apply::Remove
        );

        // A permission request for Claude Code's own question and plan tools reads as what it is.
        let ask = HookEvent {
            tool_name: Some("AskUserQuestion".into()),
            ..fixture("permission_request")
        };
        assert!(
            matches!(apply(Some(&w), &ask, 10_000), Apply::Write(f) if f.reason == "a question")
        );
        let plan = HookEvent {
            tool_name: Some("ExitPlanMode".into()),
            ..fixture("permission_request")
        };
        assert!(
            matches!(apply(Some(&w), &plan, 10_000), Apply::Write(f) if f.reason == "a plan to review")
        );

        // The backstop notification doesn't blur an exact permission reason.
        let Apply::Write(asked) = apply(Some(&w), &fixture("permission_request"), 10_000) else {
            return;
        };
        assert_eq!(
            apply(
                Some(&asked),
                &fixture("notification_permission_prompt"),
                16_000
            ),
            Apply::Keep
        );

        // Answering it shows as the next tool event.
        let Apply::Write(back) = apply(Some(&asked), &fixture("post_tool_use"), 20_000) else {
            return;
        };
        assert_eq!(back.state, State::Working);
        assert_eq!(
            back.turn_started_ms,
            Some(1_000),
            "still the turn that began with the prompt"
        );

        let Apply::Write(bg) = apply(Some(&w), &fixture("stop_background"), 10_000) else {
            return;
        };
        assert_eq!(bg.background_tasks, 2);
    }

    #[test]
    fn out_of_order_events_keep_newest() {
        let newer = working(5_000);
        assert_eq!(apply(Some(&newer), &fixture("stop"), 4_999), Apply::Keep);
        assert!(matches!(
            apply(Some(&newer), &fixture("stop"), 5_000),
            Apply::Write(_)
        ));
    }

    #[test]
    fn turn_start_and_since() {
        let Apply::Write(prompt) = apply(None, &fixture("user_prompt_submit"), 1_000) else {
            return;
        };
        let Apply::Write(tool) = apply(Some(&prompt), &fixture("pre_tool_use_bash"), 9_000) else {
            return;
        };
        assert_eq!(tool.turn_started_ms, Some(1_000));
        assert_eq!(tool.since_ms, 1_000, "still working since the prompt");
        let Apply::Write(done) = apply(Some(&tool), &fixture("stop"), 30_000) else {
            return;
        };
        assert_eq!(done.since_ms, 30_000);
        assert_eq!(done.turn_started_ms, None);
    }

    #[test]
    fn dead_process_removes_session() {
        let mut file = working(1_000);
        file.claude = Some(ProcessIdentity {
            pid: 42,
            start_time_secs: 7,
        });
        assert!(derive(&file, 2_000, Some(true), 0).is_some());
        assert!(derive(&file, 2_000, Some(false), 0).is_none());
        // Without a way to check, it goes after 12 hours of nothing.
        file.claude = None;
        assert!(derive(&file, 1_000 + STALE_MS, None, 0).is_some());
        assert!(derive(&file, 2_000 + STALE_MS, None, 0).is_none());
    }

    #[test]
    fn no_news_is_said_not_hidden() {
        let file = working(0);
        let busy = derive(&file, 4 * 60 * 1_000, Some(true), 0).map(|r| r.detail);
        assert_eq!(busy.as_deref(), Some("working · 4 min"));
        let quiet = derive(&file, 7 * 60 * 1_000, Some(true), 0).map(|r| r.detail);
        assert_eq!(quiet.as_deref(), Some("working · no news for 7 min"));
        // A transcript still being written counts as news.
        let writing = derive(&file, 7 * 60 * 1_000, Some(true), 6 * 60 * 1_000).map(|r| r.detail);
        assert_eq!(writing.as_deref(), Some("working · 7 min"));
    }

    #[test]
    fn rows_name_the_project() {
        let row = derive(&working(0), 1_000, Some(true), 0);
        assert_eq!(row.map(|r| r.project).as_deref(), Some("samo"));
        assert_eq!(duration(3_725_000), "1 h 2 min");
        assert_eq!(duration(-5), "0 s");
    }

    #[test]
    fn session_ids_are_safe_file_names() {
        assert!(valid_session_id("8f14e45f-ceea-4c2f-9f1d-2a6b0c3d4e5f"));
        assert!(!valid_session_id("../../evil"));
        assert!(!valid_session_id(""));
        assert!(!valid_session_id("a/b"));
    }
}
