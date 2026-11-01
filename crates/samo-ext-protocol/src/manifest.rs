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
