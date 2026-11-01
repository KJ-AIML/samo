//! When Claude Sessions pops up (ADR-002 C1): when a session starts waiting for you, and when a turn of at least
//! 15 s finishes. At most one of each kind per session every 30 s. Pure: the time is passed in.

use crate::sessions::{SessionFile, State};
use samo_ext_protocol::PopupKind;
use std::collections::HashMap;

/// A turn shorter than this finishes without a pop-up: you were probably watching.
pub const FINISHED_MIN_MS: i64 = 15_000;
/// The same kind of pop-up for the same session at most this often.
pub const REPEAT_MS: i64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    pub key: String,
    pub kind: PopupKind,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wanted {
    pub needs_you: bool,
    pub finished: bool,
}

#[derive(Debug, Default)]
pub struct Policy {
    last: HashMap<(String, u8), i64>,
}

/// Why a session waits, as a sentence: `permission: Bash` reads "Wants permission to use Bash".
pub fn waiting_sentence(reason: &str) -> String {
    match reason {
        "a question" => "Has a question for you".into(),
        "a plan to review" => "Has a plan for you to review".into(),
        "input needed" => "Needs your input".into(),
        "permission" => "Wants your permission".into(),
        other => match other.strip_prefix("permission: ") {
            Some(tool) => format!("Wants permission to use {tool}"),
            None => {
                let mut chars = other.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_else(|| "Is waiting for you".into())
            }
        },
    }
}

fn project(file: &SessionFile) -> String {
    file.cwd
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("A session")
        .to_string()
}

impl Policy {
    /// The pop-up for a session going from `prev` to `next`, if any.
    pub fn decide(
        &mut self,
        prev: Option<&SessionFile>,
        next: &SessionFile,
        now_ms: i64,
        wanted: Wanted,
    ) -> Option<Popup> {
        let prev = prev?;
        let name = project(next);
        let (slot, kind, title, body) = if next.state == State::NeedsYou
            && prev.state != State::NeedsYou
        {
            if !wanted.needs_you {
                return None;
            }
            (
                0u8,
                PopupKind::NeedsYou,
                name,
                waiting_sentence(&next.reason),
            )
        } else if next.state == State::Done && prev.state != State::Done && next.error {
            if !wanted.needs_you {
                return None;
            }
            (
                0u8,
                PopupKind::NeedsYou,
                name,
                "Stopped on an API error".to_string(),
            )
        } else if next.state == State::Done && prev.state != State::Done && next.reason == "done" {
            let started = prev.turn_started_ms?;
            let took = next.since_ms - started;
            if !wanted.finished || took < FINISHED_MIN_MS {
                return None;
            }
            (
                1u8,
                PopupKind::Finished,
                name,
                format!("Finished after {}", crate::sessions::duration(took)),
            )
        } else {
            return None;
        };
        let key = (next.session_id.clone(), slot);
        if self.last.get(&key).is_some_and(|t| now_ms - t < REPEAT_MS) {
            return None;
        }
        self.last.insert(key, now_ms);
        Some(Popup {
            key: format!(
                "{}-{}",
                next.session_id,
                if slot == 0 { "needs-you" } else { "finished" }
            ),
            kind,
            title,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Wanted = Wanted {
        needs_you: true,
        finished: true,
    };

    fn file(state: State, reason: &str, since: i64, turn: Option<i64>) -> SessionFile {
        SessionFile {
            session_id: "s1".into(),
            state,
            reason: reason.into(),
            error: false,
            cwd: "/Users/kj/Developer/samo".into(),
            transcript_path: String::new(),
            updated_at_ms: since,
            since_ms: since,
            turn_started_ms: turn,
            background_tasks: 0,
            claude: None,
        }
    }

    #[test]
    fn popup_policy_once_per_kind_per_30s() {
        let mut policy = Policy::default();
        let working = file(State::Working, "", 0, Some(0));
        let waiting = file(State::NeedsYou, "permission: Bash", 1_000, Some(0));
        let first = policy.decide(Some(&working), &waiting, 1_000, ALL);
        assert_eq!(first.as_ref().map(|p| p.title.as_str()), Some("samo"));
        assert_eq!(
            first.map(|p| p.body),
            Some("Wants permission to use Bash".to_string())
        );
        // Answered, then waiting again within 30 s: no second pop-up.
        assert_eq!(policy.decide(Some(&working), &waiting, 20_000, ALL), None);
        assert!(
            policy
                .decide(Some(&working), &waiting, 31_001, ALL)
                .is_some()
        );
        // Staying in the same state never pops up again.
        assert_eq!(policy.decide(Some(&waiting), &waiting, 90_000, ALL), None);
        // No baseline (Samo just started): nothing pops up for what was already so.
        assert_eq!(policy.decide(None, &waiting, 200_000, ALL), None);
    }

    #[test]
    fn finished_only_after_15s_turn() {
        let mut policy = Policy::default();
        let working = file(State::Working, "", 0, Some(0));
        let quick = file(State::Done, "done", 10_000, None);
        assert_eq!(policy.decide(Some(&working), &quick, 10_000, ALL), None);
        let slow = file(State::Done, "done", 20_000, None);
        let p = policy.decide(Some(&working), &slow, 20_000, ALL);
        assert_eq!(p.as_ref().map(|p| p.kind), Some(PopupKind::Finished));
        assert_eq!(p.map(|p| p.body), Some("Finished after 20 s".to_string()));
        // Interrupting it yourself isn't news.
        let interrupted = file(State::Done, "interrupted", 60_000, None);
        assert_eq!(
            Policy::default().decide(Some(&working), &interrupted, 60_000, ALL),
            None
        );
    }

    #[test]
    fn switches_turn_kinds_off() {
        let working = file(State::Working, "", 0, Some(0));
        let waiting = file(State::NeedsYou, "a question", 1_000, Some(0));
        let slow = file(State::Done, "done", 20_000, None);
        let none = Wanted {
            needs_you: false,
            finished: false,
        };
        assert_eq!(
            Policy::default().decide(Some(&working), &waiting, 1_000, none),
            None
        );
        assert_eq!(
            Policy::default().decide(Some(&working), &slow, 20_000, none),
            None
        );
        let mut failed = slow.clone();
        failed.error = true;
        failed.reason = "stopped: API error".into();
        let p = Policy::default().decide(Some(&working), &failed, 20_000, ALL);
        assert_eq!(
            p.map(|p| p.body),
            Some("Stopped on an API error".to_string())
        );
    }

    #[test]
    fn waiting_reads_as_a_sentence() {
        assert_eq!(
            waiting_sentence("permission: Bash"),
            "Wants permission to use Bash"
        );
        assert_eq!(waiting_sentence("permission"), "Wants your permission");
        assert_eq!(waiting_sentence("a question"), "Has a question for you");
        assert_eq!(
            waiting_sentence("a plan to review"),
            "Has a plan for you to review"
        );
        assert_eq!(waiting_sentence("input needed"), "Needs your input");
        assert_eq!(waiting_sentence("something new"), "Something new");
        assert_eq!(waiting_sentence(""), "Is waiting for you");
    }
}
