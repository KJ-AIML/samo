//! Claude Sessions' page and badge, as protocol v1 widgets (ADR-002 C1–C3). Pure: everything shown is passed in,
//! including how to write a time of day.

use crate::limits::{Limits, reading};
use crate::sessions::{Row, State};
use crate::usage::Tokens;
use samo_ext_protocol::{Lamp, NoteKind, RowItem, Widget};
use samo_primitives::{Reading, UnsupportedReason};

/// The switches the page shows, as they are now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Switches {
    pub connected: bool,
    pub plan_limits: bool,
    /// Someone else's status line is set, so Plan limits can't be turned on.
    pub other_statusline: bool,
    pub popup_needs_you: bool,
    pub popup_finished: bool,
}

pub struct Input<'a> {
    /// Each session with its tokens today.
    pub rows: &'a [(Row, Option<Tokens>)],
    pub tokens: Reading<f64>,
    pub split: Tokens,
    /// Today's transcripts are still being read, so the figure isn't complete yet.
    pub counting: bool,
    pub limits: Option<Limits>,
    pub switches: Switches,
    /// The last thing that went wrong (a setting that couldn't be changed), said once on the page.
    pub problem: Option<String>,
    pub now_ms: i64,
    /// Writes a Unix time in ms as a time of day (`18:40`).
    pub clock: &'a dyn Fn(i64) -> String,
}

/// `950`, `12.3k`, `1.23M`.
pub fn compact(n: u64) -> String {
    let f = n as f64;
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}k", f / 1_000.0)
    } else if n < 1_000_000_000 {
        format!("{:.2}M", f / 1_000_000.0)
    } else {
        format!("{:.2}G", f / 1_000_000_000.0)
    }
}

fn order(state: State) -> u8 {
    match state {
        State::NeedsYou => 0,
        State::Working => 1,
        State::Done => 2,
    }
}

/// Waiting sessions first, then working, then done.
pub fn sorted(rows: &[(Row, Option<Tokens>)]) -> Vec<&(Row, Option<Tokens>)> {
    let mut sorted: Vec<_> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        order(a.0.state)
            .cmp(&order(b.0.state))
            .then_with(|| a.0.project.cmp(&b.0.project))
    });
    sorted
}

/// How many sessions wait for you, and the quick panel's line: `Claude · 1 needs you · 2 working`.
pub fn badge(rows: &[(Row, Option<Tokens>)]) -> (u32, String) {
    let count = |s: State| rows.iter().filter(|(r, _)| r.state == s).count();
    let (waiting, working, done) = (
        count(State::NeedsYou),
        count(State::Working),
        count(State::Done),
    );
    let mut parts = vec!["Claude".to_string()];
    if waiting > 0 {
        parts.push(format!("{waiting} needs you"));
    }
    if working > 0 {
        parts.push(format!("{working} working"));
    }
    if waiting == 0 && working == 0 {
        parts.push(if done > 0 {
            format!("{done} done")
        } else {
            "no sessions".to_string()
        });
    }
    (
        u32::try_from(waiting).unwrap_or(u32::MAX),
        parts.join(" · "),
    )
}

fn token_note(tokens: &Reading<f64>, split: Tokens) -> String {
    match tokens {
        Reading::Unsupported {
            reason: UnsupportedReason::SourceNotFound,
        } => "Claude Code's transcripts weren't found in ~/.claude/projects".into(),
        Reading::Unsupported {
            reason: UnsupportedReason::FormatChanged,
        } => "Claude Code's transcript format changed; Samo can't count them".into(),
        Reading::Available { .. } => format!(
            "in {} · out {} · cache {} written, {} read",
            compact(split.input),
            compact(split.output),
            compact(split.cache_write),
            compact(split.cache_read)
        ),
        _ => String::new(),
    }
}

fn limit_meter(
    label: &str,
    window: Option<&crate::limits::Window>,
    limits: &Limits,
    input: &Input,
) -> Widget {
    let fraction = reading(window, limits.observed_at_ms, input.now_ms);
    let note = match (&fraction, window) {
        (Reading::Available { .. }, Some(w)) => format!(
            "resets {} · as of {}",
            (input.clock)(w.resets_at.saturating_mul(1_000)),
            (input.clock)(limits.observed_at_ms)
        ),
        (Reading::Stopped { .. }, Some(w)) => format!(
            "reset at {}; waiting for Claude Code to report again",
            (input.clock)(w.resets_at.saturating_mul(1_000))
        ),
        _ => "not reported by Claude Code yet".to_string(),
    };
    Widget::Meter {
        label: label.into(),
        fraction,
        note: Some(note),
    }
}

pub fn build(input: &Input) -> Vec<Widget> {
    let s = input.switches;
    let mut view = Vec::new();
    let running = input
        .rows
        .iter()
        .filter(|(r, _)| r.state != State::Done)
        .count();

    view.push(Widget::Section {
        title: "SESSIONS · LIVE".into(),
        about: Some(format!("{running} running")),
    });
    if !s.connected {
        view.push(Widget::Note {
            text: "Connect to Claude Code below, and its sessions show up here.".into(),
            kind: NoteKind::Plain,
        });
    } else if input.rows.is_empty() {
        view.push(Widget::Note {
            text: "No Claude Code sessions right now. Start one in a terminal and it appears here."
                .into(),
            kind: NoteKind::Plain,
        });
    } else {
        view.push(Widget::Rows {
            items: sorted(input.rows)
                .into_iter()
                .map(|(row, tokens)| RowItem {
                    lamp: match (row.state, row.error) {
                        (_, true) => Lamp::Error,
                        (State::Working, _) => Lamp::Working,
                        (State::NeedsYou, _) => Lamp::NeedsYou,
                        (State::Done, _) => Lamp::Done,
                    },
                    title: row.project.clone(),
                    detail: row.detail.clone(),
                    trailing: tokens.map(|t| compact(t.total())).unwrap_or_default(),
                    tag: None,
                })
                .collect(),
        });
    }

    view.push(Widget::Section {
        title: "TODAY".into(),
        about: None,
    });
    view.push(Widget::Figure {
        label: "Tokens".into(),
        value: input.tokens.clone(),
        unit: "tokens".into(),
        note: Some(if input.counting {
            "Counting today's transcripts…".to_string()
        } else {
            token_note(&input.tokens, input.split)
        })
        .filter(|n| !n.is_empty()),
    });

    view.push(Widget::Section {
        title: "PLAN LIMITS".into(),
        about: None,
    });
    match (s.plan_limits, input.limits) {
        (true, Some(limits)) => {
            view.push(limit_meter("5-hour", limits.five_hour.as_ref(), &limits, input));
            view.push(limit_meter("Weekly", limits.seven_day.as_ref(), &limits, input));
            view.push(Widget::Note {
                text: "Limits come from Claude Code's status line, which runs in a terminal. Its VS Code \
                       panel and desktop app don't run it, so limits update when you use Claude Code in a \
                       terminal."
                    .into(),
                kind: NoteKind::Plain,
            });
        }
        (true, None) => view.push(Widget::Note {
            text: "Waiting for Claude Code to report your limits. They come with its next reply in a terminal \
                   (Pro and Max plans)."
                .into(),
            kind: NoteKind::Plain,
        }),
        (false, _) => view.push(Widget::Note {
            text: "Turn on Plan limits below to see your 5-hour and weekly use.".into(),
            kind: NoteKind::Plain,
        }),
    }

    view.push(Widget::Section {
        title: "CONNECTION".into(),
        about: None,
    });
    view.push(Widget::Switch {
        key: "connect".into(),
        label: "Connect to Claude Code".into(),
        on: s.connected,
        note: Some(
            "Adds Samo's hooks to ~/.claude/settings.json, backed up first. Turning it off takes exactly them out."
                .into(),
        ),
    });
    view.push(Widget::Switch {
        key: "plan_limits".into(),
        label: "Plan limits".into(),
        on: s.plan_limits,
        note: Some(if s.other_statusline {
            "You already use a status line, and Samo never replaces it.".into()
        } else {
            "Sets Claude Code's status line to show your limits. Claude Code then hides some footer hints."
                .into()
        }),
    });
    view.push(Widget::Switch {
        key: "popup_needs_you".into(),
        label: "Pop up when a session needs you".into(),
        on: s.popup_needs_you,
        note: None,
    });
    view.push(Widget::Switch {
        key: "popup_finished".into(),
        label: "Pop up when a turn of 15 s or more finishes".into(),
        on: s.popup_finished,
        note: None,
    });
    if let Some(problem) = &input.problem {
        view.push(Widget::Note {
            text: problem.clone(),
            kind: NoteKind::Error,
        });
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::Window;

    fn row(project: &str, state: State, detail: &str) -> Row {
        Row {
            session_id: format!("id-{project}"),
            project: project.into(),
            state,
            error: false,
            detail: detail.into(),
        }
    }

    fn tokens(n: u64) -> Option<Tokens> {
        Some(Tokens {
            input: n,
            output: 0,
            cache_write: 0,
            cache_read: 0,
        })
    }

    #[test]
    fn view_for_fixture_sessions_matches_golden() -> Result<(), String> {
        let rows = vec![
            (
                row("samo", State::Working, "working · 4 min"),
                tokens(12_345),
            ),
            (
                row("talvyn-mesh", State::NeedsYou, "permission: Bash · 12 s"),
                tokens(950),
            ),
            (row("crm", State::Done, "done · 3 min ago"), None),
        ];
        let clock = |ms: i64| format!("t{}", ms / 1_000);
        let input = Input {
            rows: &rows,
            tokens: Reading::available(1_234_567.0, 5),
            counting: false,
            split: Tokens {
                input: 1_000,
                output: 20_000,
                cache_write: 200_000,
                cache_read: 1_013_567,
            },
            limits: Some(Limits {
                five_hour: Some(Window {
                    used_percentage: 34.0,
                    resets_at: 2_000,
                }),
                seven_day: None,
                observed_at_ms: 1_000_000,
            }),
            switches: Switches {
                connected: true,
                plan_limits: true,
                other_statusline: false,
                popup_needs_you: true,
                popup_finished: false,
            },
            problem: None,
            now_ms: 1_500_000,
            clock: &clock,
        };
        let actual = serde_json::to_string_pretty(&build(&input)).map_err(|e| e.to_string())?;
        let golden_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/view_golden.json"
        );
        if std::env::var("UPDATE_GOLDEN").is_ok() {
            std::fs::write(golden_path, &actual).map_err(|e| e.to_string())?;
        }
        let golden = std::fs::read_to_string(golden_path).map_err(|e| e.to_string())?;
        assert_eq!(
            actual, golden,
            "run with UPDATE_GOLDEN=1 to accept a deliberate change"
        );

        assert_eq!(
            badge(&rows),
            (1, "Claude · 1 needs you · 1 working".to_string())
        );
        Ok(())
    }

    #[test]
    fn badge_words() {
        assert_eq!(badge(&[]), (0, "Claude · no sessions".to_string()));
        let done = vec![
            (row("a", State::Done, ""), None),
            (row("b", State::Done, ""), None),
        ];
        assert_eq!(badge(&done), (0, "Claude · 2 done".to_string()));
        assert_eq!(compact(950), "950");
        assert_eq!(compact(12_345), "12.3k");
        assert_eq!(compact(1_234_567), "1.23M");
    }
}
