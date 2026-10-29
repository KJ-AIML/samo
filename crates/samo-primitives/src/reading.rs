use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedReason {
    SensorUnavailable,
    PermissionDenied,
    PlatformNotSupported,
    NotSampledYet,
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
