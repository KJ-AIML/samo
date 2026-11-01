//! Reading and writing `~/.claude/settings.json` safely (ADR-002 R7): through a symlink to its target, via a
//! temporary file and a rename, only if nobody changed it since it was read, and backed up once first.

use serde_json::{Map, Value};
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Where the backup goes, beside the settings file.
pub const BACKUP_NAME: &str = "settings.json.samo-bak";

/// `~/.claude/settings.json`.
pub fn settings_path(home: &Path) -> PathBuf {
    home.join(".claude").join("settings.json")
}

/// What the file held when it was read (`None`: it didn't exist), to tell whether it changed since.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(Option<Vec<u8>>);

impl Fingerprint {
    pub fn existed(&self) -> bool {
        self.0.is_some()
    }
}

/// The file the path leads to: a symlink's target (dotfile managers link `settings.json`), or the path itself.
fn target(path: &Path) -> PathBuf {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("couldn't read {}: {e}", path.display())),
    }
}

/// Reads the settings. A missing or empty file is `{}`; a file that isn't JSON is an error (Samo won't touch
/// it).
pub fn read(path: &Path) -> Result<(Value, Fingerprint), String> {
    let bytes = read_bytes(&target(path))?;
    let value = match &bytes {
        None => Value::Object(Map::new()),
        Some(b) if b.iter().all(u8::is_ascii_whitespace) => Value::Object(Map::new()),
        Some(b) => serde_json::from_slice(b).map_err(|e| {
            format!(
                "{} isn't valid JSON ({e}); Samo won't change it",
                path.display()
            )
        })?,
    };
    Ok((value, Fingerprint(bytes)))
}

/// Writes `value` as 2-space JSON, only if the file still holds what `read` saw.
pub fn write_if_unchanged(path: &Path, value: &Value, seen: &Fingerprint) -> Result<(), String> {
    let target = target(path);
    if read_bytes(&target)? != seen.0 {
        return Err(format!(
            "{} changed while Samo was editing it; try again",
            path.display()
        ));
    }
    let dir = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))?;
    let mut text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    text.push('\n');
    let tmp = dir.join(format!(".settings.json.samo-{}", std::process::id()));
    fs::write(&tmp, text).map_err(|e| format!("couldn't write {}: {e}", tmp.display()))?;
    if let Ok(meta) = fs::metadata(&target) {
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(meta.permissions().mode()));
    }
    fs::rename(&tmp, &target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("couldn't replace {}: {e}", target.display())
    })
}

/// Removes the file Samo created, if it is still there.
pub fn remove(path: &Path) -> Result<(), String> {
    match fs::remove_file(target(path)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("couldn't remove {}: {e}", path.display())),
    }
}

/// Copies the settings to `settings.json.samo-bak` beside them, the first time only. `Ok(false)` if there was
/// nothing to back up or a backup already exists.
pub fn backup_once(path: &Path) -> Result<bool, String> {
    let backup = path.with_file_name(BACKUP_NAME);
    if fs::symlink_metadata(&backup).is_ok() {
        return Ok(false);
    }
    match read_bytes(&target(path))? {
        None => Ok(false),
        Some(bytes) => {
            fs::write(&backup, bytes)
                .map_err(|e| format!("couldn't back up to {}: {e}", backup.display()))?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{connect, disconnect, is_connected};
    use std::os::unix::fs::symlink;

    #[test]
    fn write_refused_if_changed_since_read() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = settings_path(dir.path());
        fs::create_dir_all(dir.path().join(".claude")).map_err(|e| e.to_string())?;
        fs::write(&path, r#"{"model":"opus"}"#).map_err(|e| e.to_string())?;

        let (value, seen) = read(&path)?;
        // Claude Code writes in between.
        fs::write(&path, r#"{"model":"sonnet"}"#).map_err(|e| e.to_string())?;
        let (connected, _) = connect(value, true).map_err(|e| e.to_string())?;
        assert!(write_if_unchanged(&path, &connected, &seen).is_err());
        assert_eq!(
            fs::read_to_string(&path).map_err(|e| e.to_string())?,
            r#"{"model":"sonnet"}"#
        );
        Ok(())
    }

    #[test]
    fn symlinked_settings_stay_a_symlink() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let dotfiles = dir.path().join("dotfiles");
        fs::create_dir_all(&dotfiles).map_err(|e| e.to_string())?;
        fs::create_dir_all(dir.path().join(".claude")).map_err(|e| e.to_string())?;
        let real = dotfiles.join("claude-settings.json");
        fs::write(&real, r#"{"model":"opus"}"#).map_err(|e| e.to_string())?;
        let path = settings_path(dir.path());
        symlink(&real, &path).map_err(|e| e.to_string())?;

        let (value, seen) = read(&path)?;
        let (connected, _) = connect(value, seen.existed()).map_err(|e| e.to_string())?;
        write_if_unchanged(&path, &connected, &seen)?;

        assert!(
            fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
        );
        let (through_link, _) = read(&path)?;
        assert!(is_connected(&through_link));
        let (direct, _) = read(&real)?;
        assert_eq!(direct, through_link);
        Ok(())
    }

    #[test]
    fn backup_written_once() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = settings_path(dir.path());
        assert_eq!(backup_once(&path), Ok(false), "nothing to back up yet");
        fs::create_dir_all(dir.path().join(".claude")).map_err(|e| e.to_string())?;
        fs::write(&path, "{\"first\":1}").map_err(|e| e.to_string())?;
        assert_eq!(backup_once(&path), Ok(true));
        fs::write(&path, "{\"second\":2}").map_err(|e| e.to_string())?;
        assert_eq!(backup_once(&path), Ok(false));
        let backup =
            fs::read_to_string(path.with_file_name(BACKUP_NAME)).map_err(|e| e.to_string())?;
        assert_eq!(backup, "{\"first\":1}");
        Ok(())
    }

    #[test]
    fn missing_settings_file_is_created_and_removed_cleanly() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = settings_path(dir.path());
        let (value, seen) = read(&path)?;
        assert!(!seen.existed());
        let (connected, created) = connect(value, seen.existed()).map_err(|e| e.to_string())?;
        write_if_unchanged(&path, &connected, &seen)?;
        assert!(path.is_file());
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        assert!(
            text.starts_with("{\n  \"hooks\": {"),
            "2-space JSON: {text}"
        );

        let (value, seen) = read(&path)?;
        let (back, delete) = disconnect(value, Some(&created));
        assert!(delete);
        assert_eq!(back, Value::Object(Map::new()));
        let _ = seen;
        remove(&path)?;
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn invalid_json_is_left_alone() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = settings_path(dir.path());
        fs::create_dir_all(dir.path().join(".claude")).map_err(|e| e.to_string())?;
        fs::write(&path, "{ not json").map_err(|e| e.to_string())?;
        assert!(read(&path).is_err());
        Ok(())
    }
}
