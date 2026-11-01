use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedReason {
    SensorUnavailable,
    PermissionDenied,
    PlatformNotSupported,
    NotSampledYet,
    /// Where the value comes from isn't there (e.g. another app's files weren't found).
    SourceNotFound,
    /// The source is there but its format is no longer one Samo can read.
    FormatChanged,
    /// The source exists and is readable, but it didn't report this value.
    NotReported,
}

/// R3: Every metric reading carries an explicit availability state.
/// Samo never reports 0 when a sensor is unavailable, and Stopped leaves an honest gap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Reading<T> {
    Available { value: T, sampled_at_ms: i64 },
    Unsupported { reason: UnsupportedReason },
    Stopped { since_ms: i64 },
}

impl<T> Reading<T> {
    pub fn available(value: T, sampled_at_ms: i64) -> Self {
        Self::Available {
            value,
            sampled_at_ms,
        }
    }

    pub fn unsupported(reason: UnsupportedReason) -> Self {
        Self::Unsupported { reason }
    }

    pub fn stopped(since_ms: i64) -> Self {
        Self::Stopped { since_ms }
    }

    pub fn as_value(&self) -> Option<&T> {
        match self {
            Self::Available { value, .. } => Some(value),
            Self::Unsupported { .. } | Self::Stopped { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_unsupported_reasons_round_trip_snake_case() {
        for (reason, wire) in [
            (UnsupportedReason::SourceNotFound, "\"source_not_found\""),
            (UnsupportedReason::FormatChanged, "\"format_changed\""),
            (UnsupportedReason::NotReported, "\"not_reported\""),
        ] {
            let json = serde_json::to_string(&reason).map_err(|e| e.to_string());
            assert_eq!(json.as_deref(), Ok(wire));
            let back: Result<UnsupportedReason, _> = serde_json::from_str(wire);
            assert_eq!(back.ok(), Some(reason));
        }
        let reading: Reading<f64> = Reading::unsupported(UnsupportedReason::NotReported);
        let json = serde_json::to_string(&reading).map_err(|e| e.to_string());
        assert_eq!(
            json.as_deref(),
            Ok("{\"state\":\"unsupported\",\"reason\":\"not_reported\"}")
        );
    }
}
