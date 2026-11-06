//! Claude Sessions' page and badge, as protocol v1 widgets (ADR-002 C1–C3). Pure: everything shown is passed in,
//! including how to write a time of day.

use crate::limits::{Limits, observed_at, reading};
use crate::sessions::{Row, State};
use crate::usage::Tokens;
use samo_ext_protocol::{BadgeMeter, Lamp, NoteKind, RowItem, Widget};
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
    /// `limits` holds a report (from the status line), not only limits reached: a window it leaves out isn't
    /// applied to the account right now.
    pub reported: bool,
    pub switches: Switches,
    /// The last thing that went wrong (a setting that couldn't be changed), said once on the page.
    pub problem: Option<String>,
    pub now_ms: i64,
    /// Writes a Unix time in ms as a time (`18:40`, or `Thu 22:00` on another day).
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

/// The quick panel's load bar, while Plan limits is on: the 5-hour window's use, or the weekly's while Anthropic
/// applies no 5-hour limit (KJ's account near its weekly limit, 2026-11-06). Unknown until Claude Code reports
/// it, and after it resets.
pub fn badge_meter(plan_limits: bool, limits: Option<&Limits>, now_ms: i64) -> Option<BadgeMeter> {
    plan_limits.then(|| match limits {
        Some(l) if l.five_hour.is_none() && l.seven_day.is_some() => BadgeMeter {
            label: "wk".into(),
            fraction: reading(l.seven_day.as_ref(), l.observed_at_ms, now_ms),
        },
        Some(l) => BadgeMeter {
            label: "5h".into(),
            fraction: reading(l.five_hour.as_ref(), l.observed_at_ms, now_ms),
        },
        None => BadgeMeter {
            label: "5h".into(),
            fraction: Reading::unsupported(UnsupportedReason::NotReported),
        },
    })
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

/// Past 10 minutes, how old a figure is: Claude Code may have been busy elsewhere without reporting.
fn age(ms: i64) -> String {
    let minutes = ms / 60_000;
    match minutes {
        ..10 => String::new(),
        10..60 => format!(", {minutes} min ago"),
        60..2_880 => format!(", {} h ago", minutes / 60),
        _ => format!(", {} days ago", minutes / 1_440),
    }
}

/// One window's meter. `name` is how its note says it: `5-hour`, `weekly`.
fn limit_meter(
    label: &str,
    name: &str,
    window: Option<&crate::limits::Window>,
    limits: &Limits,
    input: &Input,
) -> Widget {
    let fraction = reading(window, limits.observed_at_ms, input.now_ms);
    let note = match (&fraction, window) {
        (Reading::Available { .. }, Some(w)) => {
            let resets = (input.clock)(w.resets_at.saturating_mul(1_000));
            let seen = observed_at(w, limits.observed_at_ms);
            if w.used_percentage >= 100.0 {
                format!("limit reached at {} · resets {resets}", (input.clock)(seen))
            } else {
                format!(
                    "resets {resets} · as of {}{}",
                    (input.clock)(seen),
                    age(input.now_ms - seen)
                )
            }
        }
        (Reading::Stopped { .. }, Some(w)) => format!(
            "reset at {}; waiting for Claude Code to report again",
            (input.clock)(w.resets_at.saturating_mul(1_000))
        ),
        // Claude Code reports a window whenever Anthropic sends it, so a report without it means there is none.
        (_, None) if input.reported => format!(
            "no {name} limit right now · as of {}",
            (input.clock)(limits.observed_at_ms)
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
                    id: None,
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
                    actions: Vec::new(),
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
            view.push(limit_meter(
                "5-hour",
                "5-hour",
                limits.five_hour.as_ref(),
                &limits,
                input,
            ));
            view.push(limit_meter(
                "Weekly",
                "weekly",
                limits.seven_day.as_ref(),
                &limits,
                input,
            ));
            view.push(Widget::Note {
                text: "Claude Code reports your limits only when it replies in a terminal. In Cursor, VS \
                       Code, the desktop app or claude -p it doesn't, so the figures say how old they are. A \
                       limit reached in any session shows here at once."
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
            reported: true,
            limits: Some(Limits {
                five_hour: Some(Window {
                    used_percentage: 34.0,
                    resets_at: 2_000,
                    observed_at_ms: None,
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
    fn old_figures_say_their_age_and_a_limit_reached_says_so() -> Result<(), String> {
        let minute = 60_000;
        let now = 1_000 * minute;
        let clock = |ms: i64| format!("t{}", ms / minute);
        let limits = Limits {
            five_hour: Some(Window {
                used_percentage: 100.0,
                resets_at: 1_100 * 60,
                observed_at_ms: Some(now - 2 * minute),
            }),
            seven_day: Some(Window {
                used_percentage: 15.0,
                resets_at: 9_000 * 60,
                observed_at_ms: None,
            }),
            observed_at_ms: now - 125 * minute,
        };
        let input = Input {
            rows: &[],
            tokens: Reading::unsupported(UnsupportedReason::NotReported),
            counting: false,
            split: Tokens::default(),
            limits: Some(limits),
            reported: true,
            switches: Switches {
                plan_limits: true,
                ..Switches::default()
            },
            problem: None,
            now_ms: now,
            clock: &clock,
        };
        let notes: Vec<String> = build(&input)
            .into_iter()
            .filter_map(|w| match w {
                Widget::Meter { note, .. } => note,
                _ => None,
            })
            .collect();
        assert_eq!(
            notes,
            vec![
                "limit reached at t998 · resets t1100".to_string(),
                "resets t9000 · as of t875, 2 h ago".to_string(),
            ]
        );
        assert_eq!(age(9 * minute), "");
        assert_eq!(age(10 * minute), ", 10 min ago");
        assert_eq!(age(3 * 1_440 * minute), ", 3 days ago");
        Ok(())
    }

    #[test]
    fn badge_meter_follows_plan_limits() {
        let limits = Limits {
            five_hour: Some(Window {
                used_percentage: 36.0,
                resets_at: 2_000,
                observed_at_ms: None,
            }),
            seven_day: None,
            observed_at_ms: 1_000_000,
        };
        assert_eq!(badge_meter(false, Some(&limits), 1_500_000), None);
        let on = badge_meter(true, Some(&limits), 1_500_000);
        assert_eq!(on.as_ref().map(|m| m.label.as_str()), Some("5h"));
        assert_eq!(
            on.map(|m| m.fraction),
            Some(Reading::available(0.36, 1_000_000))
        );
        // Before Claude Code reports, and after the window resets: unknown, not 0.
        assert_eq!(
            badge_meter(true, None, 1).map(|m| m.fraction),
            Some(Reading::unsupported(UnsupportedReason::NotReported))
        );
        assert_eq!(
            badge_meter(true, Some(&limits), 2_000_001).map(|m| m.fraction),
            Some(Reading::stopped(2_000_000))
        );
    }

    /// KJ's report of 2026-11-06 22:47: near the weekly limit, Anthropic sent no 5-hour window at all.
    #[test]
    fn a_report_without_a_window_says_there_is_none() {
        let minute = 60_000;
        let now = 1_000 * minute;
        let clock = |ms: i64| format!("t{}", ms / minute);
        let weekly_only = Limits {
            five_hour: None,
            seven_day: Some(Window {
                used_percentage: 94.0,
                resets_at: 9_000 * 60,
                observed_at_ms: None,
            }),
            observed_at_ms: now - 2 * minute,
        };
        let notes = |limits: Limits, reported: bool| -> Vec<String> {
            let input = Input {
                rows: &[],
                tokens: Reading::unsupported(UnsupportedReason::NotReported),
                counting: false,
                split: Tokens::default(),
                limits: Some(limits),
                reported,
                switches: Switches {
                    plan_limits: true,
                    ..Switches::default()
                },
                problem: None,
                now_ms: now,
                clock: &clock,
            };
            build(&input)
                .into_iter()
                .filter_map(|w| match w {
                    Widget::Meter { note, .. } => note,
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            notes(weekly_only, true),
            vec![
                "no 5-hour limit right now · as of t998".to_string(),
                "resets t9000 · as of t998".to_string(),
            ]
        );
        // Only a limit reached, from a transcript, with no report: nothing is known about the other window.
        let reached = Limits {
            five_hour: Some(Window {
                used_percentage: 100.0,
                resets_at: 1_100 * 60,
                observed_at_ms: Some(now - minute),
            }),
            seven_day: None,
            observed_at_ms: now - minute,
        };
        assert_eq!(
            notes(reached, false)[1],
            "not reported by Claude Code yet".to_string()
        );

        // The quick panel shows the weekly while there's no 5-hour limit.
        let meter = badge_meter(true, Some(&weekly_only), now);
        assert_eq!(meter.as_ref().map(|m| m.label.as_str()), Some("wk"));
        assert_eq!(
            meter.map(|m| m.fraction),
            Some(Reading::available(0.94, now - 2 * minute))
        );
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
