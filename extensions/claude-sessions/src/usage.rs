//! Today's tokens from Claude Code's transcripts (ADR-002 C2). Pure: lines go in, a tally comes out.
//!
//! The transcript format is internal to Claude Code and may change. Today an assistant line carries
//! `message.id`, `sessionId`, `timestamp` and `message.usage` (`input_tokens`, `output_tokens`,
//! `cache_creation_input_tokens`, `cache_read_input_tokens`). One message is written as several lines that repeat
//! its usage, and a resumed session can copy history into a new file, so each `message.id` counts once a day.
//! When most usage lines can't be read, the figure says the format changed rather than showing a wrong number.

use samo_primitives::{Reading, UnsupportedReason};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_write)
            .saturating_add(self.cache_read)
    }

    fn add(&mut self, other: Tokens) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Usage {
        msg_id: String,
        session_id: String,
        ts_ms: i64,
        tokens: Tokens,
    },
    /// Anything without usage.
    Other,
    /// It mentions usage but can't be read: a sign the format changed.
    Bad,
}

pub fn parse_line(text: &str) -> Line {
    let mentions_usage = text.contains("\"usage\"");
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return if mentions_usage {
            Line::Bad
        } else {
            Line::Other
        };
    };
    let Some(usage) = v.pointer("/message/usage") else {
        return Line::Other;
    };
    let Some(usage) = usage.as_object() else {
        return Line::Bad;
    };
    let field = |name: &str| usage.get(name).and_then(Value::as_u64);
    let counts = [
        field("input_tokens"),
        field("output_tokens"),
        field("cache_creation_input_tokens"),
        field("cache_read_input_tokens"),
    ];
    if counts.iter().all(Option::is_none) {
        return Line::Bad;
    }
    let msg_id = v
        .pointer("/message/id")
        .and_then(Value::as_str)
        .or_else(|| v.get("requestId").and_then(Value::as_str));
    let ts_ms = v
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.timestamp_millis());
    let (Some(msg_id), Some(ts_ms)) = (msg_id, ts_ms) else {
        return Line::Bad;
    };
    Line::Usage {
        msg_id: msg_id.to_string(),
        session_id: v
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        ts_ms,
        tokens: Tokens {
            input: counts[0].unwrap_or(0),
            output: counts[1].unwrap_or(0),
            cache_write: counts[2].unwrap_or(0),
            cache_read: counts[3].unwrap_or(0),
        },
    }
}

/// Today's tokens, each message counted once.
#[derive(Debug, Clone, Default)]
pub struct DayTally {
    midnight_ms: i64,
    seen: HashSet<String>,
    total: Tokens,
    per_session: HashMap<String, Tokens>,
    usage_lines: u64,
    bad_lines: u64,
    /// Claude Code's transcripts folder was found.
    pub source_found: bool,
}

/// Below this many usage lines, unreadable ones don't yet mean the format changed.
const FORMAT_SAMPLE: u64 = 10;

impl DayTally {
    pub fn new(midnight_ms: i64) -> Self {
        Self {
            midnight_ms,
            ..Self::default()
        }
    }

    pub fn midnight_ms(&self) -> i64 {
        self.midnight_ms
    }

    /// Counts a line from `fallback_session`'s transcript (a line's own `sessionId` wins: a subagent's
    /// transcript counts toward the session that started it).
    pub fn add(&mut self, fallback_session: &str, line: Line) {
        match line {
            Line::Other => {}
            Line::Bad => {
                self.usage_lines += 1;
                self.bad_lines += 1;
            }
            Line::Usage {
                msg_id,
                session_id,
                ts_ms,
                tokens,
            } => {
                self.usage_lines += 1;
                if ts_ms < self.midnight_ms || !self.seen.insert(msg_id) {
                    return;
                }
                self.total.add(tokens);
                let session = if session_id.is_empty() {
                    fallback_session.to_string()
                } else {
                    session_id
                };
                self.per_session.entry(session).or_default().add(tokens);
            }
        }
    }

    pub fn total(&self) -> Tokens {
        self.total
    }

    pub fn per_session(&self, session_id: &str) -> Option<Tokens> {
        self.per_session.get(session_id).copied()
    }

    /// The day's total as a reading: unknown (never `0`) when the transcripts weren't found or can't be read.
    pub fn reading(&self, now_ms: i64) -> Reading<f64> {
        if !self.source_found {
            return Reading::unsupported(UnsupportedReason::SourceNotFound);
        }
        if self.usage_lines >= FORMAT_SAMPLE && self.bad_lines * 2 > self.usage_lines {
            return Reading::unsupported(UnsupportedReason::FormatChanged);
        }
        Reading::available(self.total.total() as f64, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage_line(msg: &str, session: &str, ts: &str, input: u64, output: u64) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"{session}","requestId":"req_{msg}","timestamp":"{ts}","message":{{"id":"{msg}","usage":{{"input_tokens":{input},"output_tokens":{output},"cache_creation_input_tokens":10,"cache_read_input_tokens":100,"service_tier":"standard"}}}}}}"#
        )
    }

    const MIDNIGHT: &str = "2026-11-01T00:00:00Z";

    fn midnight_ms() -> i64 {
        chrono::DateTime::parse_from_rfc3339(MIDNIGHT).map_or(0, |t| t.timestamp_millis())
    }

    #[test]
    fn message_id_counted_once() {
        let mut tally = DayTally::new(midnight_ms());
        tally.source_found = true;
        // One message written as three lines, then copied into a resumed session's file.
        for _ in 0..3 {
            tally.add(
                "s1",
                parse_line(&usage_line("msg_1", "s1", "2026-11-01T09:00:00Z", 5, 50)),
            );
        }
        tally.add(
            "s2",
            parse_line(&usage_line("msg_1", "s2", "2026-11-01T09:00:00Z", 5, 50)),
        );
        tally.add(
            "s1",
            parse_line(&usage_line("msg_2", "s1", "2026-11-01T09:01:00Z", 1, 2)),
        );
        let t = tally.total();
        assert_eq!(
            (t.input, t.output, t.cache_write, t.cache_read),
            (6, 52, 20, 200)
        );
        assert_eq!(t.total(), 278);
        assert_eq!(tally.per_session("s1").map(|t| t.total()), Some(278));
        assert_eq!(tally.per_session("s2"), None);
        assert_eq!(tally.reading(1), Reading::available(278.0, 1));
    }

    #[test]
    fn lines_before_midnight_ignored() {
        let mut tally = DayTally::new(midnight_ms());
        tally.source_found = true;
        tally.add(
            "s",
            parse_line(&usage_line(
                "old",
                "s",
                "2026-10-31T23:59:59Z",
                1_000,
                1_000,
            )),
        );
        tally.add(
            "s",
            parse_line(&usage_line("new", "s", "2026-11-01T00:00:01Z", 1, 1)),
        );
        assert_eq!(tally.total().total(), 112);
    }

    #[test]
    fn unparseable_transcript_is_unsupported_not_zero() {
        let mut tally = DayTally::new(midnight_ms());
        tally.source_found = true;
        for i in 0..12 {
            tally.add(
                "s",
                parse_line(&format!(r#"{{"message":{{"usage":"renamed {i}"}}}}"#)),
            );
        }
        tally.add(
            "s",
            parse_line(&usage_line("m", "s", "2026-11-01T09:00:00Z", 1, 1)),
        );
        assert_eq!(
            tally.reading(1),
            Reading::unsupported(UnsupportedReason::FormatChanged)
        );

        // A few odd lines among good ones don't hide the figure.
        let mut mostly_fine = DayTally::new(midnight_ms());
        mostly_fine.source_found = true;
        for i in 0..20 {
            mostly_fine.add(
                "s",
                parse_line(&usage_line(
                    &format!("m{i}"),
                    "s",
                    "2026-11-01T09:00:00Z",
                    1,
                    1,
                )),
            );
        }
        mostly_fine.add("s", parse_line(r#"{"message":{"usage":7}}"#));
        assert!(matches!(mostly_fine.reading(1), Reading::Available { .. }));
    }

    #[test]
    fn missing_folder_is_source_not_found() {
        let tally = DayTally::new(midnight_ms());
        assert_eq!(
            tally.reading(1),
            Reading::unsupported(UnsupportedReason::SourceNotFound)
        );
    }

    #[test]
    fn lines_without_usage_are_ignored() {
        assert_eq!(
            parse_line(r#"{"type":"user","message":{"role":"user"}}"#),
            Line::Other
        );
        assert_eq!(parse_line("{ half a line"), Line::Other);
        assert_eq!(parse_line(r#"{ "usage": broken"#), Line::Bad);
        assert_eq!(
            parse_line(r#"{"message":{"usage":{"input_tokens":1}}}"#),
            Line::Bad,
            "no id or time"
        );
    }
}
