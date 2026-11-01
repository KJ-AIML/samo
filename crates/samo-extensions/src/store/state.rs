//! The replay floor (R11): the newest `published_at` accepted, per catalog URL, in
//! `~/Library/Application Support/Samo/store-state.toml`. Not in the cache, which macOS may purge.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
struct StateFile {
    #[serde(default)]
    floors: BTreeMap<String, String>,
}

pub struct Floors {
    file: PathBuf,
}

impl Floors {
    pub fn at(file: &Path) -> Self {
        Self {
            file: file.to_path_buf(),
        }
    }

    fn read(&self) -> StateFile {
        fs::read_to_string(&self.file)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn get(&self, url: &str) -> Option<String> {
        self.read().floors.get(url).cloned()
    }

    /// Raises the floor for `url` to `published_at`; never lowers it.
    pub fn raise(&self, url: &str, published_at: &str) -> Result<(), String> {
        let mut state = self.read();
        let newer = match state.floors.get(url) {
            Some(old) => samo_domain::store::check_replay(old, Some(published_at)).is_err(),
            None => true,
        };
        if !newer {
            return Ok(());
        }
        state.floors.insert(url.into(), published_at.into());
        let text = toml::to_string(&state).map_err(|e| e.to_string())?;
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = self.file.with_extension("toml.tmp");
        fs::write(&tmp, text).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.file).map_err(|e| e.to_string())
    }
}
