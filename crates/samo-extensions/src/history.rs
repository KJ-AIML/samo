use crate::ExtRoot;
use std::fs::{self, OpenOptions};
use std::io::Write;

/// `history.log`: one JSON line per install, update, remove, and per change an extension reports (`log`).
pub struct History;

impl History {
    /// Appends one line. Best effort: failing to write history never stops what it records.
    pub fn append(root: &ExtRoot, event: &str, id: &str, detail: &str) {
        let line = serde_json::json!({
            "at": chrono::Local::now().to_rfc3339(),
            "event": event,
            "id": id,
            "detail": detail,
        });
        let _ = fs::create_dir_all(root.path());
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.history_file())
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_appends_json_lines() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let root = ExtRoot::at(dir.path().join("Extensions"));
        History::append(&root, "install", "hello", "dev /tmp/hello");
        History::append(
            &root,
            "connect",
            "claude-sessions",
            "line\nbreak \"quoted\"",
        );
        let text = fs::read_to_string(root.history_file()).map_err(|e| e.to_string())?;
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let second: serde_json::Value =
            serde_json::from_str(lines[1]).map_err(|e| e.to_string())?;
        assert_eq!(second["event"], "connect");
        assert_eq!(second["id"], "claude-sessions");
        assert_eq!(second["detail"], "line\nbreak \"quoted\"");
        assert!(second["at"].as_str().is_some_and(|a| a.contains('T')));
        Ok(())
    }
}
