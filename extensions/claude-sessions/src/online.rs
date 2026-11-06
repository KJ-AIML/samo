//! Plan limits from Anthropic, while **Ask Anthropic for limits** is on (ADR-002 C3, amended 2026-11-06).
//!
//! Claude Code reports limits only to a terminal's status line, so in Cursor, VS Code, the desktop app or
//! `claude -p` they go stale. With the switch on, `serve` asks Anthropic's usage page every 5 minutes, as Claude
//! Code's `/usage` does, with Claude Code's own sign-in:
//!
//! - The sign-in is only read: on the Mac from the Keychain item `Claude Code-credentials` with `security`, as
//!   Claude Code reads it, else from `~/.claude/.credentials.json`. It is never refreshed, written or kept: once it
//!   has expired, Samo waits for Claude Code to renew it, so it can never sign Claude Code out.
//! - One address: `https://api.anthropic.com/api/oauth/usage`, no redirects, 15 s at most, 256 KiB at most.
//! - A sign-in Anthropic refused isn't sent again; after any other failure Samo waits 15 minutes.
//!
//! This is the only file in Claude Sessions that may go online (the `network` gate checks).

use crate::limits::{Limits, Window};
use serde_json::Value;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use ureq::Agent;

pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// How often Samo asks while the switch is on.
pub const EVERY: Duration = Duration::from_secs(5 * 60);
/// How long Samo waits after Anthropic couldn't be reached or answered with an error.
pub const AFTER_FAILURE: Duration = Duration::from_secs(15 * 60);
const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BODY: u64 = 256 * 1024;
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const SECURITY_TIMEOUT: Duration = Duration::from_secs(5);

/// Why Samo didn't get the limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// No sign-in of Claude Code's was found: it isn't signed in with a Claude account.
    NoSignIn,
    /// Claude Code's sign-in has expired; Claude Code renews it the next time it's used.
    Expired,
    /// Anthropic refused this sign-in (401 or 403). The fingerprint lets Samo wait for a new one.
    Refused { sign_in: u64 },
    /// Anthropic couldn't be reached, or answered something else.
    Failed(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::NoSignIn => write!(f, "Claude Code isn't signed in with a Claude account"),
            Problem::Expired => write!(f, "Claude Code's sign-in has expired"),
            Problem::Refused { .. } => write!(f, "Anthropic didn't accept Claude Code's sign-in"),
            Problem::Failed(why) => write!(f, "{why}"),
        }
    }
}

/// Claude Code's access token, and a fingerprint of it that is safe to keep.
pub struct SignIn {
    token: String,
    pub fingerprint: u64,
}

impl fmt::Debug for SignIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SignIn({:x})", self.fingerprint)
    }
}

/// The access token in Claude Code's credentials (`claudeAiOauth.accessToken`), while it hasn't expired.
pub fn sign_in_from(credentials: &[u8], now_ms: i64) -> Result<SignIn, Problem> {
    let v: Value = serde_json::from_slice(credentials).map_err(|_| Problem::NoSignIn)?;
    let oauth = v.get("claudeAiOauth").ok_or(Problem::NoSignIn)?;
    let token = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or(Problem::NoSignIn)?;
    // A minute's margin, so a request isn't sent with a token about to lapse.
    if let Some(expires) = oauth.get("expiresAt").and_then(Value::as_i64)
        && now_ms + 60_000 >= expires
    {
        return Err(Problem::Expired);
    }
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    Ok(SignIn {
        token: token.to_string(),
        fingerprint: hasher.finish(),
    })
}

/// `2026-11-08T15:00:00.668483+00:00` as Unix seconds.
fn unix_seconds(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.timestamp())
}

/// A window: `Some(None)` when it is `null` (not applied to the account right now), `None` when it is missing or
/// unreadable.
fn window(v: &Value, name: &str) -> Option<Option<Window>> {
    let w = v.get(name)?;
    if w.is_null() {
        return Some(None);
    }
    let used = w.get("utilization")?.as_f64()?;
    let resets_at = unix_seconds(w.get("resets_at")?.as_str()?)?;
    used.is_finite().then_some(Some(Window {
        used_percentage: used,
        resets_at,
        observed_at_ms: None,
    }))
}

/// The limits in the usage page's answer. A window that is `null` isn't applied to the account right now. If either
/// is missing or can't be read, the page has changed and nothing is said: a window is never made up as 0 or none.
pub fn limits_from_usage(v: &Value, now_ms: i64) -> Option<Limits> {
    Some(Limits {
        five_hour: window(v, "five_hour")?,
        seven_day: window(v, "seven_day")?,
        observed_at_ms: now_ms,
    })
}

/// Claude Code's sign-in, read as Claude Code reads it.
fn read_sign_in(home: &Path, now_ms: i64) -> Result<SignIn, Problem> {
    if cfg!(target_os = "macos")
        && let Some(found) = from_keychain()
    {
        return sign_in_from(&found, now_ms);
    }
    let file = std::fs::read(home.join(".claude").join(".credentials.json"))
        .map_err(|_| Problem::NoSignIn)?;
    sign_in_from(&file, now_ms)
}

/// `security find-generic-password -a <user> -w -s "Claude Code-credentials"`, given 5 s.
fn from_keychain() -> Option<Vec<u8>> {
    let mut command = Command::new("/usr/bin/security");
    command.arg("find-generic-password");
    if let Some(user) = std::env::var_os("USER").filter(|u| !u.is_empty()) {
        command.arg("-a").arg(user);
    }
    let mut child = command
        .args(["-w", "-s", KEYCHAIN_SERVICE])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if started.elapsed() >= SECURITY_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut out = Vec::new();
    child.stdout.take()?.read_to_end(&mut out).ok()?;
    Some(out)
}

/// Asks Anthropic once. `skip` is the fingerprint of a sign-in it already refused, which isn't sent again.
pub fn ask(home: &Path, skip: Option<u64>, now_ms: impl Fn() -> i64) -> Result<Limits, Problem> {
    let sign_in = read_sign_in(home, now_ms())?;
    if skip == Some(sign_in.fingerprint) {
        return Err(Problem::Refused {
            sign_in: sign_in.fingerprint,
        });
    }
    let agent: Agent = Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .https_only(true)
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();
    let response = agent
        .get(USAGE_URL)
        .header("Authorization", &format!("Bearer {}", sign_in.token))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Content-Type", "application/json")
        .header(
            "User-Agent",
            &format!("samo-claude/{}", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| Problem::Failed(format!("couldn't reach Anthropic ({e})")))?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => {
            return Err(Problem::Refused {
                sign_in: sign_in.fingerprint,
            });
        }
        429 => return Err(Problem::Failed("Anthropic asked Samo to wait".into())),
        code => return Err(Problem::Failed(format!("Anthropic answered {code}"))),
    }
    let body = response
        .into_body()
        .with_config()
        .limit(MAX_BODY)
        .read_to_vec()
        .map_err(|e| Problem::Failed(format!("couldn't read Anthropic's answer ({e})")))?;
    let v: Value = serde_json::from_slice(&body)
        .map_err(|_| Problem::Failed("Anthropic's answer wasn't JSON".into()))?;
    limits_from_usage(&v, now_ms())
        .ok_or_else(|| Problem::Failed("Anthropic's answer has changed; Samo can't read it".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_793_980_000_000;

    #[test]
    fn the_sign_in_is_read_and_an_expired_one_is_not_used() {
        let creds = |expires: i64| {
            json!({"claudeAiOauth": {"accessToken": "sk-ant-oat01-x", "refreshToken": "r", "expiresAt": expires,
                "subscriptionType": "max"}, "mcpOAuth": {}})
            .to_string()
        };
        let ok = sign_in_from(creds(NOW + 3_600_000).as_bytes(), NOW);
        assert!(matches!(&ok, Ok(s) if s.token == "sk-ant-oat01-x"));
        // The token never shows in a debug print.
        assert!(ok.is_ok_and(|s| !format!("{s:?}").contains("sk-ant")));
        assert_eq!(
            sign_in_from(creds(NOW + 30_000).as_bytes(), NOW).err(),
            Some(Problem::Expired)
        );
        assert_eq!(sign_in_from(b"{}", NOW).err(), Some(Problem::NoSignIn));
        assert_eq!(
            sign_in_from(b"not json", NOW).err(),
            Some(Problem::NoSignIn)
        );
        let same = |t: &str| {
            sign_in_from(
                json!({"claudeAiOauth": {"accessToken": t}})
                    .to_string()
                    .as_bytes(),
                NOW,
            )
            .map(|s| s.fingerprint)
        };
        assert_eq!(same("a"), same("a"));
        assert_ne!(same("a"), same("b"));
    }

    /// The usage page's answer on KJ's Mac, 2026-11-06 22:58 (trimmed): no 5-hour limit, weekly at 95%.
    #[test]
    fn the_usage_page_is_read() {
        let kj = json!({
            "five_hour": null,
            "seven_day": {"utilization": 95.0, "resets_at": "2026-11-08T15:00:00.668483+00:00",
                "limit_dollars": null},
            "seven_day_opus": null,
            "limits": [{"kind": "weekly_all", "group": "weekly", "percent": 95}]
        });
        let l = limits_from_usage(&kj, NOW);
        assert_eq!(
            l,
            Some(Limits {
                five_hour: None,
                seven_day: Some(Window {
                    used_percentage: 95.0,
                    resets_at: 1_794_150_000,
                    observed_at_ms: None,
                }),
                observed_at_ms: NOW,
            })
        );
        let both = json!({
            "five_hour": {"utilization": 34.0, "resets_at": "2026-11-06T18:00:00+00:00"},
            "seven_day": {"utilization": 12.5, "resets_at": "2026-11-08T15:00:00Z"}
        });
        let l = limits_from_usage(&both, NOW);
        assert_eq!(
            l.and_then(|l| l.five_hour).map(|w| w.used_percentage),
            Some(34.0)
        );
        assert_eq!(
            l.and_then(|l| l.seven_day).map(|w| w.resets_at),
            Some(1_794_150_000)
        );
        // Not the page Samo knows: nothing is said about either window.
        assert_eq!(limits_from_usage(&json!({"error": "x"}), NOW), None);
        // A window Samo can't read isn't made 0 or none: the answer is set aside.
        let odd =
            json!({"five_hour": {"utilization": "34", "resets_at": "soon"}, "seven_day": null});
        assert_eq!(limits_from_usage(&odd, NOW), None);
    }
}
