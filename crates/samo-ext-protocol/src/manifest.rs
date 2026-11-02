use crate::{PROTOCOL_VERSION, ProtocolError};
use serde::{Deserialize, Serialize};

/// `extension.toml`, at the top of every package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Lower-case letters, digits and `-`; also the extension's folder name.
    pub id: String,
    pub name: String,
    /// Also a folder name, so only `[0-9A-Za-z.+-]`.
    pub version: String,
    pub protocol: u32,
    /// The oldest Samo that can run it.
    pub min_samo: String,
    /// The helper program, relative to the package folder.
    pub helper: String,
    /// One line, shown in the Extensions list.
    pub summary: String,
    /// Everything it changes outside its own folder, shown before it is installed (R7).
    #[serde(default)]
    pub changes: Vec<Change>,
    /// The switches its page may show.
    #[serde(default)]
    pub settings: Vec<SettingDecl>,
    /// It opens its own windows, and says which (ADR-004, R15). Without this it may not open any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<String>,
    /// macOS permissions it asks for, from [`KNOWN_PERMISSIONS`]; shown before install.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
    /// The oldest macOS it runs on, e.g. `"14.0"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_macos: Option<String>,
    /// More than R8's defaults, declared and shown before install (R17).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<Limits>,
}

/// The permissions an extension may declare.
pub const KNOWN_PERMISSIONS: &[&str] = &["screen-recording"];
/// The most memory an extension may declare.
pub const MAX_MEMORY_MB: u32 = 1024;
/// The most CPU an extension may declare: one whole core, averaged over a minute.
pub const MAX_CPU_PCT: u32 = 100;

/// An extension's own limits, in place of R8's 150 MB and 10 % of a core.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_pct: Option<u32>,
}

/// One thing an extension changes on the Mac, in words a person reads before installing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub what: String,
    #[serde(rename = "where")]
    pub place: String,
    pub undo: String,
}

/// A switch an extension's page may show; the helper owns its state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingDecl {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub default: bool,
}

/// `[a-z0-9-]`, 1 to 40 characters, not starting or ending with `-`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `[0-9A-Za-z.+-]`, 1 to 32 characters, and not `.` or `..`.
pub fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 32
        && version != "."
        && version != ".."
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-'))
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 40
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A macOS version: 1 to 3 dot-separated numbers.
pub fn valid_macos(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    (1..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// A relative path that stays inside the package: no leading `/`, no empty, `.` or `..` part.
fn stays_inside(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, ProtocolError> {
        let manifest: Self =
            toml::from_str(text).map_err(|e| ProtocolError::Manifest(e.message().to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        let bad = |why: String| Err(ProtocolError::Manifest(why));
        if !valid_id(&self.id) {
            return bad(format!("id `{}` must be 1-40 of a-z, 0-9 and -", self.id));
        }
        if !valid_version(&self.version) {
            return bad(format!("version `{}` is not a plain version", self.version));
        }
        if self.protocol != PROTOCOL_VERSION {
            return bad(format!(
                "it speaks protocol {}, this Samo speaks {PROTOCOL_VERSION}",
                self.protocol
            ));
        }
        if self.name.trim().is_empty() {
            return bad("name is empty".into());
        }
        if !stays_inside(&self.helper) {
            return bad(format!(
                "helper `{}` must be a path inside the package",
                self.helper
            ));
        }
        if self.changes.is_empty() {
            return bad("it must say what it changes on the Mac ([[changes]])".into());
        }
        if let Some(s) = self.settings.iter().find(|s| !valid_key(&s.key)) {
            return bad(format!("setting key `{}` must be a-z, 0-9 and _", s.key));
        }
        if self.windows.as_ref().is_some_and(|w| w.trim().is_empty()) {
            return bad("windows must say which windows it opens".into());
        }
        if let Some(p) = self
            .permissions
            .iter()
            .find(|p| !KNOWN_PERMISSIONS.contains(&p.as_str()))
        {
            return bad(format!("permission `{p}` isn't one Samo knows"));
        }
        if let Some(v) = self.min_macos.as_ref().filter(|v| !valid_macos(v)) {
            return bad(format!("min_macos `{v}` is not a macOS version"));
        }
        if let Some(limits) = &self.limits {
            if limits
                .memory_mb
                .is_some_and(|m| m == 0 || m > MAX_MEMORY_MB)
            {
                return bad(format!("memory_mb must be 1 to {MAX_MEMORY_MB}"));
            }
            if limits.cpu_pct.is_some_and(|c| c == 0 || c > MAX_CPU_PCT) {
                return bad(format!("cpu_pct must be 1 to {MAX_CPU_PCT}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
id = "claude-sessions"
name = "Claude Sessions"
version = "0.1.0"
protocol = 1
min_samo = "0.1.0"
helper = "bin/samo-claude"
summary = "Claude Code sessions at a glance"

[[changes]]
what = "Adds hooks to Claude Code's settings when you press Connect"
where = "~/.claude/settings.json"
undo = "Disconnect, Off or Remove takes them out"

[[settings]]
key = "connect"
label = "Connect to Claude Code"
"#;

    #[test]
    fn parses_a_good_manifest() -> Result<(), String> {
        let m = Manifest::parse(GOOD).map_err(|e| e.to_string())?;
        assert_eq!(m.id, "claude-sessions");
        assert_eq!(m.changes[0].place, "~/.claude/settings.json");
        assert_eq!(m.settings[0].key, "connect");
        assert!(!m.settings[0].default);
        Ok(())
    }

    #[test]
    fn manifest_rejects_bad_id_escaping_helper_and_empty_changes() {
        let swap = |from: &str, to: &str| Manifest::parse(&GOOD.replace(from, to));
        assert!(swap("id = \"claude-sessions\"", "id = \"Claude\"").is_err());
        assert!(swap("id = \"claude-sessions\"", "id = \"../x\"").is_err());
        assert!(swap("id = \"claude-sessions\"", "id = \"-x\"").is_err());
        assert!(swap("bin/samo-claude", "../bin/samo-claude").is_err());
        assert!(swap("bin/samo-claude", "/usr/bin/true").is_err());
        assert!(swap("bin/samo-claude", "bin//x").is_err());
        assert!(swap("version = \"0.1.0\"", "version = \"../0\"").is_err());
        assert!(swap("protocol = 1", "protocol = 2").is_err());
        assert!(swap("key = \"connect\"", "key = \"Connect Now\"").is_err());

        let no_changes = GOOD
            .split("[[changes]]")
            .next()
            .map(str::to_string)
            .unwrap_or_default();
        assert!(Manifest::parse(&no_changes).is_err());
    }

    const SHOTS: &str = r#"
id = "shots"
name = "Shots"
version = "0.1.0"
protocol = 1
min_samo = "0.3.0"
helper = "bin/samo-shots"
summary = "Screenshots"
windows = "Opens its own windows: the capture overlay"
permissions = ["screen-recording"]
min_macos = "14.0"

[limits]
memory_mb = 800
cpu_pct = 60

[[changes]]
what = "Saves screenshots"
where = "~/Pictures/Shots"
undo = "Nothing to undo"
"#;

    #[test]
    fn manifest_capabilities_parse_and_validate() -> Result<(), String> {
        let m = Manifest::parse(SHOTS).map_err(|e| e.to_string())?;
        assert_eq!(
            m.windows.as_deref(),
            Some("Opens its own windows: the capture overlay")
        );
        assert_eq!(m.permissions, ["screen-recording"]);
        assert_eq!(m.min_macos.as_deref(), Some("14.0"));
        assert_eq!(
            m.limits,
            Some(Limits {
                memory_mb: Some(800),
                cpu_pct: Some(60)
            })
        );

        // Without them, nothing is declared.
        let plain = Manifest::parse(GOOD).map_err(|e| e.to_string())?;
        assert_eq!(
            (
                plain.windows,
                plain.permissions.len(),
                plain.min_macos,
                plain.limits
            ),
            (None, 0, None, None)
        );

        let swap = |from: &str, to: &str| Manifest::parse(&SHOTS.replace(from, to));
        assert!(swap("\"screen-recording\"", "\"camera\"").is_err());
        assert!(swap("memory_mb = 800", "memory_mb = 2048").is_err());
        assert!(swap("memory_mb = 800", "memory_mb = 0").is_err());
        assert!(swap("cpu_pct = 60", "cpu_pct = 150").is_err());
        assert!(swap("min_macos = \"14.0\"", "min_macos = \"fourteen\"").is_err());
        assert!(swap("\"Opens its own windows: the capture overlay\"", "\" \"").is_err());
        assert!(valid_macos("14") && valid_macos("14.0") && valid_macos("15.1.2"));
        assert!(!valid_macos("14.") && !valid_macos("1.2.3.4") && !valid_macos(""));
        Ok(())
    }

    #[test]
    fn ids_and_versions() {
        assert!(valid_id("hello"));
        assert!(valid_id("claude-sessions"));
        assert!(!valid_id(""));
        assert!(!valid_id(&"a".repeat(41)));
        assert!(!valid_id("a_b"));
        assert!(valid_version("0.1.0"));
        assert!(valid_version("1.0.0-beta+3"));
        assert!(!valid_version(".."));
        assert!(!valid_version("1/2"));
    }
}
