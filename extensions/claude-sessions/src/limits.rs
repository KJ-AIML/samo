//! Plan limits from Claude Code's status line (ADR-002 C3). Claude Code documents `rate_limits.five_hour` and
//! `rate_limits.seven_day`, each with `used_percentage` and `resets_at` (Unix seconds), for Pro and Max plans
//! and only after the first reply. Pure: the time is passed in.

use samo_primitives::{Reading, UnsupportedReason};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub used_percentage: f64,
    /// Unix seconds.
    pub resets_at: i64,
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
}

/// A window's share used, `0..=1`. Not reported is unknown; once its reset time has passed, the old figure no
/// longer holds, so it is stopped until Claude Code reports again.
pub fn reading(window: Option<&Window>, observed_at_ms: i64, now_ms: i64) -> Reading<f64> {
    match window {
        None => Reading::unsupported(UnsupportedReason::NotReported),
        Some(w) if now_ms >= w.resets_at.saturating_mul(1_000) => {
            Reading::stopped(w.resets_at.saturating_mul(1_000))
        }
        Some(w) => Reading::available((w.used_percentage / 100.0).clamp(0.0, 1.0), observed_at_ms),
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
