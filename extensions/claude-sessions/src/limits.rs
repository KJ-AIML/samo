//! Plan limits from Claude Code's status line (ADR-002 C3). Claude Code documents `rate_limits.five_hour` and
//! `rate_limits.seven_day`, each with `used_percentage` and `resets_at` (Unix seconds), for Pro and Max plans
//! and only after the first reply. A report can leave a window out (KJ's had the 5-hour one without the weekly),
//! so the last one seen is kept until it resets.
//!
//! Only a terminal runs the status line: Claude Code in Cursor, VS Code, the desktop app or `claude -p` never reports
//! limits. Every session, though, writes a refusal for a limit to its transcript (`apiErrorParams.rate_limit_info`,
//! status `rejected`), so a limit reached anywhere is shown at once. Pure: the time is passed in.

use samo_primitives::{Reading, UnsupportedReason};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub used_percentage: f64,
    /// Unix seconds.
    pub resets_at: i64,
    /// When it was reported, if before the rest: kept from an earlier report that had it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub observed_at_ms: i64,
}

fn window(v: &Value, name: &str) -> Option<Window> {
    let w = v.pointer(&format!("/rate_limits/{name}"))?;
    let used = w.get("used_percentage")?.as_f64()?;
    let resets = w.get("resets_at")?;
    let resets_at = resets
        .as_i64()
        .or_else(|| resets.as_f64().map(|f| f as i64))?;
    used.is_finite().then_some(Window {
        used_percentage: used,
        resets_at,
        observed_at_ms: None,
    })
}

/// The limits in what Claude Code gave its status line.
pub fn from_status_json(v: &Value, now_ms: i64) -> Limits {
    Limits {
        five_hour: window(v, "five_hour"),
        seven_day: window(v, "seven_day"),
        observed_at_ms: now_ms,
    }
}

impl Limits {
    pub fn any(&self) -> bool {
        self.five_hour.is_some() || self.seven_day.is_some()
    }

    /// This report, with each window it leaves out taken from `previous` while that one hasn't reset.
    pub fn keeping(self, previous: Option<&Limits>) -> Limits {
        let Some(prev) = previous else {
            return self;
        };
        let now = self.observed_at_ms;
        let kept = |new: Option<Window>, old: Option<Window>| {
            new.or_else(|| {
                old.filter(|w| now < w.resets_at.saturating_mul(1_000))
                    .map(|w| Window {
                        observed_at_ms: Some(w.observed_at_ms.unwrap_or(prev.observed_at_ms)),
                        ..w
                    })
            })
        };
        Limits {
            five_hour: kept(self.five_hour, prev.five_hour),
            seven_day: kept(self.seven_day, prev.seven_day),
            observed_at_ms: now,
        }
    }
}

/// A session was refused for a plan limit, at `at_ms`, until `resets_at` (Unix seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub at_ms: i64,
    pub resets_at: i64,
}

/// The latest refusal for each window, from the transcripts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hits {
    pub five_hour: Option<Hit>,
    pub seven_day: Option<Hit>,
}

impl Hits {
    /// Notes a refusal from a transcript line, if it is one. The latest one of each window is kept.
    pub fn note_line(&mut self, text: &str) {
        // Rare: only a refusal carries the key; one inside a string is escaped and doesn't match.
        if !text.contains("\"rate_limit_info\"") {
            return;
        }
        let Some((window, hit)) = hit_in(text) else {
            return;
        };
        let slot = match window {
            "five_hour" => &mut self.five_hour,
            "seven_day" => &mut self.seven_day,
            _ => return,
        };
        if slot.is_none_or(|old| old.at_ms <= hit.at_ms) {
            *slot = Some(hit);
        }
    }
}

fn hit_in(text: &str) -> Option<(&'static str, Hit)> {
    let v = serde_json::from_str::<Value>(text).ok()?;
    let info = v.pointer("/apiErrorParams/rate_limit_info")?;
    if info.get("status")?.as_str()? != "rejected" {
        return None;
    }
    let window = match info.get("rateLimitType")?.as_str()? {
        "five_hour" => "five_hour",
        "seven_day" => "seven_day",
        _ => return None,
    };
    let at_ms = chrono::DateTime::parse_from_rfc3339(v.get("timestamp")?.as_str()?)
        .ok()?
        .timestamp_millis();
    Some((
        window,
        Hit {
            at_ms,
            resets_at: info.get("resetsAt")?.as_i64()?,
        },
    ))
}

/// The limits to show: the status line's, with a window replaced by a refusal that came after it and hasn't reset.
pub fn with_hits(limits: Option<Limits>, hits: &Hits, now_ms: i64) -> Option<Limits> {
    let in_force = |hit: &Option<Hit>| hit.filter(|h| now_ms < h.resets_at.saturating_mul(1_000));
    let (five, seven) = (in_force(&hits.five_hour), in_force(&hits.seven_day));
    if five.is_none() && seven.is_none() {
        return limits;
    }
    let base = limits.unwrap_or(Limits {
        five_hour: None,
        seven_day: None,
        observed_at_ms: five.or(seven).map_or(now_ms, |h| h.at_ms),
    });
    let pick = |window: Option<Window>, hit: Option<Hit>| match (window, hit) {
        (Some(w), Some(h)) if observed_at(&w, base.observed_at_ms) >= h.at_ms => Some(w),
        (_, Some(h)) => Some(Window {
            used_percentage: 100.0,
            resets_at: h.resets_at,
            observed_at_ms: Some(h.at_ms),
        }),
        (w, None) => w,
    };
    Some(Limits {
        five_hour: pick(base.five_hour, five),
        seven_day: pick(base.seven_day, seven),
        observed_at_ms: base.observed_at_ms,
    })
}

/// When `window` was reported: its own time if it was kept from an earlier report, else the report's.
pub fn observed_at(window: &Window, report_ms: i64) -> i64 {
    window.observed_at_ms.unwrap_or(report_ms)
}

/// A window's share used, `0..=1`. Not reported is unknown; once its reset time has passed, the old figure no
/// longer holds, so it is stopped until Claude Code reports again.
pub fn reading(window: Option<&Window>, observed_at_ms: i64, now_ms: i64) -> Reading<f64> {
    match window {
        None => Reading::unsupported(UnsupportedReason::NotReported),
        Some(w) if now_ms >= w.resets_at.saturating_mul(1_000) => {
            Reading::stopped(w.resets_at.saturating_mul(1_000))
        }
        Some(w) => Reading::available(
            (w.used_percentage / 100.0).clamp(0.0, 1.0),
            observed_at(w, observed_at_ms),
        ),
    }
}

/// The status line's own text: `5h 34% · wk 12%`; empty when neither is reported.
pub fn status_text(limits: &Limits) -> String {
    let part =
        |label: &str, w: &Option<Window>| w.map(|w| format!("{label} {:.0}%", w.used_percentage));
    [part("5h", &limits.five_hour), part("wk", &limits.seven_day)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn missing_window_is_not_reported() {
        let only_week = json!({"rate_limits": {"seven_day": {"used_percentage": 12, "resets_at": 1_800_100_000}}});
        let l = from_status_json(&only_week, NOW);
        assert_eq!(
            reading(l.five_hour.as_ref(), l.observed_at_ms, NOW),
            Reading::unsupported(UnsupportedReason::NotReported)
        );
        assert_eq!(
            reading(l.seven_day.as_ref(), l.observed_at_ms, NOW),
            Reading::available(0.12, NOW)
        );
        assert_eq!(status_text(&l), "wk 12%");
        let none = from_status_json(&json!({"model": {"id": "x"}}), NOW);
        assert!(!none.any());
        assert_eq!(status_text(&none), "");
    }

    #[test]
    fn limits_past_reset_are_unknown() {
        let v = json!({"rate_limits": {
            "five_hour": {"used_percentage": 34.4, "resets_at": 1_800_000_100},
            "seven_day": {"used_percentage": 12.0, "resets_at": 1_800_500_000}
        }});
        let l = from_status_json(&v, NOW);
        assert_eq!(status_text(&l), "5h 34% · wk 12%");
        let later = 1_800_000_200_000;
        assert_eq!(
            reading(l.five_hour.as_ref(), l.observed_at_ms, later),
            Reading::stopped(1_800_000_100_000)
        );
        assert!(matches!(
            reading(l.seven_day.as_ref(), l.observed_at_ms, later),
            Reading::Available { .. }
        ));
    }

    #[test]
    fn a_window_left_out_is_kept_until_it_resets() {
        let both = from_status_json(
            &json!({"rate_limits": {
                "five_hour": {"used_percentage": 30, "resets_at": 1_800_000_100},
                "seven_day": {"used_percentage": 47, "resets_at": 1_800_500_000}
            }}),
            NOW,
        );
        let later = NOW + 60_000;
        let only_5h = json!({"rate_limits": {
            "five_hour": {"used_percentage": 40, "resets_at": 1_800_018_000},
            "seven_day": null
        }});
        let l = from_status_json(&only_5h, later).keeping(Some(&both));
        assert_eq!(l.observed_at_ms, later);
        assert_eq!(l.five_hour.map(|w| w.used_percentage), Some(40.0));
        assert_eq!(l.five_hour.and_then(|w| w.observed_at_ms), None);
        // The weekly 47% stays, as of the report that had it.
        assert_eq!(
            reading(l.seven_day.as_ref(), l.observed_at_ms, later),
            Reading::available(0.47, NOW)
        );
        // Kept again, it keeps its first time.
        let again = from_status_json(&only_5h, later + 60_000).keeping(Some(&l));
        assert_eq!(again.seven_day.and_then(|w| w.observed_at_ms), Some(NOW));

        // A window past its reset isn't kept: it is unknown until Claude Code reports it.
        let after_reset = 1_800_000_200_000;
        let only_week = json!({"rate_limits": {
            "seven_day": {"used_percentage": 50, "resets_at": 1_800_500_000}
        }});
        let l = from_status_json(&only_week, after_reset).keeping(Some(&both));
        assert_eq!(l.five_hour, None);
        assert_eq!(l.seven_day.map(|w| w.used_percentage), Some(50.0));
        assert_eq!(
            from_status_json(&only_week, NOW).keeping(None).five_hour,
            None
        );
    }

    #[test]
    fn old_limits_files_still_read() -> Result<(), String> {
        let old = r#"{"five_hour":{"used_percentage":40.0,"resets_at":1793640600},"seven_day":null,"observed_at_ms":1793632683646}"#;
        let l: Limits = serde_json::from_str(old).map_err(|e| e.to_string())?;
        assert_eq!(l.five_hour.and_then(|w| w.observed_at_ms), None);
        let text = serde_json::to_string(&l).map_err(|e| e.to_string())?;
        assert_eq!(text, old);
        Ok(())
    }

    /// A refusal as Claude Code writes it (trimmed), from a session in Cursor.
    fn refusal(ts: &str, kind: &str, resets_at: i64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"id":"x","usage":{{"input_tokens":0,"output_tokens":0}}}},"apiError":"usage_limit_reached","apiErrorParams":{{"rate_limit_info":{{"status":"rejected","resetsAt":{resets_at},"rateLimitType":"{kind}","overageStatus":"rejected","isUsingOverage":false}}}},"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429,"entrypoint":"claude-vscode"}}"#
        )
    }

    #[test]
    fn refusals_are_noted_latest_first() {
        let mut hits = Hits::default();
        hits.note_line(&refusal("2026-11-03T18:00:00Z", "five_hour", 1_793_735_400));
        hits.note_line(&refusal("2026-11-03T17:00:00Z", "five_hour", 1_793_700_000));
        hits.note_line(&refusal(
            "2026-11-03T18:00:00Z",
            "seven_day_opus",
            1_793_735_400,
        ));
        hits.note_line(
            r#"{"type":"user","message":{"content":"see \"rate_limit_info\" in the docs"}}"#,
        );
        hits.note_line(r#"{"type":"assistant","usage":{}}"#);
        let at = chrono::DateTime::parse_from_rfc3339("2026-11-03T18:00:00Z")
            .map_or(0, |t| t.timestamp_millis());
        assert_eq!(
            hits.five_hour,
            Some(Hit {
                at_ms: at,
                resets_at: 1_793_735_400
            })
        );
        assert_eq!(hits.seven_day, None);
    }

    #[test]
    fn a_newer_refusal_shows_the_limit_reached() {
        let resets = 1_800_018_000;
        let hit = |at_ms| Hits {
            five_hour: Some(Hit {
                at_ms,
                resets_at: resets,
            }),
            seven_day: None,
        };
        let reported = Limits {
            five_hour: Some(Window {
                used_percentage: 34.0,
                resets_at: resets,
                observed_at_ms: None,
            }),
            seven_day: Some(Window {
                used_percentage: 15.0,
                resets_at: 1_800_500_000,
                observed_at_ms: None,
            }),
            observed_at_ms: NOW,
        };
        // Refused after the last report: reached, as of the refusal; the weekly is untouched.
        let l = with_hits(Some(reported), &hit(NOW + 60_000), NOW + 120_000);
        assert_eq!(
            reading(l.and_then(|l| l.five_hour).as_ref(), NOW, NOW + 120_000),
            Reading::available(1.0, NOW + 60_000)
        );
        assert_eq!(l.and_then(|l| l.seven_day), reported.seven_day);
        // A report after the refusal wins.
        assert_eq!(
            with_hits(Some(reported), &hit(NOW - 60_000), NOW),
            Some(reported)
        );
        // Past its reset, a refusal is forgotten.
        let after = resets * 1_000 + 1;
        assert_eq!(
            with_hits(Some(reported), &hit(NOW + 60_000), after),
            Some(reported)
        );
        // With no report at all, the refusal alone shows.
        let alone = with_hits(None, &hit(NOW), NOW + 1);
        assert_eq!(
            alone.and_then(|l| l.five_hour).map(|w| w.used_percentage),
            Some(100.0)
        );
        assert_eq!(alone.and_then(|l| l.seven_day), None);
        assert_eq!(with_hits(None, &Hits::default(), NOW), None);
    }

    #[test]
    fn odd_values_are_tolerated() {
        let v = json!({"rate_limits": {
            "five_hour": {"used_percentage": 140, "resets_at": 1_800_000_100.5},
            "seven_day": {"used_percentage": "12", "resets_at": 1}
        }});
        let l = from_status_json(&v, NOW);
        assert_eq!(
            reading(l.five_hour.as_ref(), NOW, NOW),
            Reading::available(1.0, NOW)
        );
        assert_eq!(l.seven_day, None);
    }
}
