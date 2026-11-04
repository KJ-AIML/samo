//! The history on disk (ADR-005 R23): `history.json`, written whole and atomically, and `items/` for rich text,
//! images and long text. All of it is in Clips' own data folder, which uninstalling deletes. Ported from `Store.swift`.

use crate::history::{Added, Capture, History, INLINE_TEXT, Item};
use std::fs;
use std::path::{Path, PathBuf};

/// Why the saved history wasn't loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unloaded {
    /// It was damaged, and is kept beside, by this name.
    Damaged(String),
    /// It couldn't be read, for this reason.
    Unreadable(String),
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    fn items(&self) -> PathBuf {
        self.dir.join("items")
    }

    fn file(&self) -> PathBuf {
        self.dir.join("history.json")
    }

    /// The saved history; none yet is an empty one. A damaged file is set aside (renamed, never deleted) and said;
    /// one that can't be read (another program has it open) is said, so it is never taken for empty and saved over.
    pub fn load(&self) -> Result<History, Unloaded> {
        let bytes = match fs::read(self.file()) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(History::default()),
            Err(e) => return Err(Unloaded::Unreadable(e.to_string())),
        };
        if let Ok(history) = serde_json::from_slice(&bytes) {
            return Ok(history);
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let aside = format!("history.damaged-{stamp}.json");
        fs::rename(self.file(), self.dir.join(&aside)).map_err(|e| {
            Unloaded::Unreadable(format!(
                "its history is damaged and couldn't be set aside: {e}"
            ))
        })?;
        Err(Unloaded::Damaged(aside))
    }

    /// Writes `bytes` to `to` through a temporary file beside it, so a crash never leaves half a file.
    pub(crate) fn write_atomic(to: &Path, bytes: &[u8]) -> Result<(), String> {
        let tmp = to.with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&tmp, bytes).map_err(|e| format!("couldn't save: {e}"))?;
        fs::rename(&tmp, to).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("couldn't save: {e}")
        })
    }

    pub fn save(&self, history: &History) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| format!("couldn't make Clips' folder: {e}"))?;
        let bytes = serde_json::to_vec(history).map_err(|e| e.to_string())?;
        Self::write_atomic(&self.file(), &bytes)
    }

    /// Writes an item's contents; returns its file's name in `items/`.
    fn write(&self, data: &[u8], id: &str, ext: &str) -> Result<String, String> {
        fs::create_dir_all(self.items())
            .map_err(|e| format!("couldn't make Clips' folder: {e}"))?;
        let name = format!("{id}.{ext}");
        Self::write_atomic(&self.items().join(&name), data)?;
        Ok(name)
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        // Only a plain name inside `items/`.
        if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
            return None;
        }
        fs::read(self.items().join(name)).ok()
    }

    /// Deletes an item's files.
    pub fn delete(&self, item: &Item) {
        for name in [&item.text_file, &item.data_file].into_iter().flatten() {
            if !name.contains(['/', '\\']) && !name.contains("..") {
                let _ = fs::remove_file(self.items().join(name));
            }
        }
    }

    /// Saves a new item's contents beside it: the whole text if it was cut, and its rich text or image.
    pub fn keep(
        &self,
        capture: &Capture,
        added: &Added,
        history: &mut History,
    ) -> Result<(), String> {
        if !added.is_new {
            return Ok(());
        }
        let id = &added.item.id;
        let text = if capture.text.len() > INLINE_TEXT {
            Some(self.write(capture.text.as_bytes(), id, "txt")?)
        } else {
            None
        };
        let data = match &capture.data {
            Some(bytes) => Some(self.write(bytes, id, &ext(capture.data_type.as_deref()))?),
            None => None,
        };
        history.set_files(id, text, data);
        Ok(())
    }

    /// An item's whole text, read from its file when it was too long for the list.
    pub fn full_text(&self, item: &Item) -> String {
        item.text_file
            .as_deref()
            .and_then(|name| self.read(name))
            // Valid UTF-8 (as Clips wrote it) becomes the String without another copy.
            .map(|bytes| {
                String::from_utf8(bytes)
                    .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
            })
            .unwrap_or_else(|| item.text.clone())
    }
}

/// A kept file's extension, from what it was copied as.
fn ext(data_type: Option<&str>) -> String {
    match data_type {
        Some(t @ ("png" | "dib" | "rtf" | "html")) => t.to_string(),
        _ => "bin".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{Capture, History, Kind};
    use chrono::Utc;

    #[test]
    fn the_store_round_trips_every_kind() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let store = Store::new(dir.path());
        let mut h = History::default();
        let now = Utc::now();
        let thai = "ภาษาไทย ".repeat(15_000);
        let captures = [
            Capture::text("plain", Some("notepad.exe")),
            Capture::text(&thai, None),
            Capture {
                kind: Kind::Image,
                text: String::new(),
                data: Some(vec![137, 80, 78, 71]),
                data_type: Some("png".into()),
                files: Vec::new(),
                source: None,
            },
            Capture {
                kind: Kind::Files,
                text: r"C:\a\one.txt".into(),
                data: None,
                data_type: None,
                files: vec![r"C:\a\one.txt".into()],
                source: None,
            },
        ];
        for c in captures {
            let added = h.add(&c, now);
            store.keep(&c, &added, &mut h)?;
        }
        store.save(&h)?;
        let back = store.load().map_err(|e| format!("{e:?}"))?;
        assert_eq!(back, h);
        let long = back
            .items()
            .iter()
            .find(|i| i.text_file.is_some())
            .ok_or("no long text")?;
        assert_eq!(store.full_text(long), thai);
        let image = back
            .items()
            .iter()
            .find(|i| i.kind == Kind::Image)
            .ok_or("no image")?;
        assert_eq!(
            store.read(image.data_file.as_deref().unwrap_or_default()),
            Some(vec![137, 80, 78, 71])
        );
        // Deleting an item deletes its files.
        let file = dir
            .path()
            .join("items")
            .join(long.text_file.clone().unwrap_or_default());
        assert!(file.is_file());
        store.delete(long);
        assert!(!file.exists());
        Ok(())
    }

    #[test]
    fn a_missing_history_is_empty() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let store = Store::new(&dir.path().join("none-yet"));
        assert_eq!(store.load(), Ok(History::default()));
        Ok(())
    }

    #[test]
    fn a_damaged_history_is_set_aside_not_lost() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        std::fs::write(dir.path().join("history.json"), b"{damaged").map_err(|e| e.to_string())?;
        let Err(Unloaded::Damaged(aside)) = Store::new(dir.path()).load() else {
            return Err("a damaged history wasn't set aside".into());
        };
        assert!(!dir.path().join("history.json").exists());
        let kept = std::fs::read(dir.path().join(&aside)).map_err(|e| e.to_string())?;
        assert_eq!(kept, b"{damaged");
        Ok(())
    }

    #[test]
    fn a_history_that_cant_be_read_isnt_taken_for_empty() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        // Something Clips can't read where its history is.
        std::fs::create_dir(dir.path().join("history.json")).map_err(|e| e.to_string())?;
        assert!(matches!(
            Store::new(dir.path()).load(),
            Err(Unloaded::Unreadable(_))
        ));
        Ok(())
    }
}
