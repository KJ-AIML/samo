use crate::{ExtRoot, InstallError};
use samo_domain::extensions::Source;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;

/// One installed extension, as `installed.toml` records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub id: String,
    pub version: String,
    pub on: bool,
    pub source: Source,
}

/// `installed.toml`: what is installed, in the order it was installed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    pub extensions: Vec<Installed>,
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default, rename = "extension")]
    extensions: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    id: String,
    version: String,
    on: bool,
    /// `catalog` or `dev`.
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dev_path: Option<String>,
}

impl Registry {
    /// Reads `installed.toml`; a missing file is an empty registry. Entries it can't make sense of are skipped.
    pub fn load(root: &ExtRoot) -> Result<Self, InstallError> {
        let text = match fs::read_to_string(root.registry_file()) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(InstallError::io("couldn't read installed.toml", e)),
        };
        let file: File = toml::from_str(&text)
            .map_err(|e| InstallError::Io(format!("installed.toml is damaged: {}", e.message())))?;
        let extensions = file
            .extensions
            .into_iter()
            .filter_map(|e| {
                let source = match (e.source.as_str(), e.dev_path) {
                    ("catalog", _) => Source::Catalog,
                    ("dev", Some(path)) => Source::Dev { path },
                    _ => return None,
                };
                Some(Installed {
                    id: e.id,
                    version: e.version,
                    on: e.on,
                    source,
                })
            })
            .collect();
        Ok(Self { extensions })
    }

    /// Writes `installed.toml` atomically.
    pub fn save(&self, root: &ExtRoot) -> Result<(), InstallError> {
        let file = File {
            extensions: self
                .extensions
                .iter()
                .map(|i| {
                    let (source, dev_path) = match &i.source {
                        Source::Catalog => ("catalog".to_string(), None),
                        Source::Dev { path } => ("dev".to_string(), Some(path.clone())),
                    };
                    Entry {
                        id: i.id.clone(),
                        version: i.version.clone(),
                        on: i.on,
                        source,
                        dev_path,
                    }
                })
                .collect(),
        };
        let text = toml::to_string(&file)
            .map_err(|e| InstallError::Io(format!("couldn't write installed.toml: {e}")))?;
        fs::create_dir_all(root.path())
            .map_err(|e| InstallError::io("couldn't create the Extensions folder", e))?;
        let tmp = root.path().join(".installed.toml.tmp");
        fs::write(&tmp, text).map_err(|e| InstallError::io("couldn't write installed.toml", e))?;
        fs::rename(&tmp, root.registry_file())
            .map_err(|e| InstallError::io("couldn't write installed.toml", e))
    }

    pub fn get(&self, id: &str) -> Option<&Installed> {
        self.extensions.iter().find(|i| i.id == id)
    }

    /// Adds `installed`, or replaces the entry with its id in place.
    pub fn upsert(&mut self, installed: Installed) {
        match self.extensions.iter_mut().find(|i| i.id == installed.id) {
            Some(slot) => *slot = installed,
            None => self.extensions.push(installed),
        }
    }

    pub fn set_on(&mut self, id: &str, on: bool) {
        if let Some(i) = self.extensions.iter_mut().find(|i| i.id == id) {
            i.on = on;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_round_trip() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let root = ExtRoot::at(dir.path().join("Extensions"));
        assert_eq!(
            Registry::load(&root).map_err(|e| e.to_string())?,
            Registry::default()
        );

        let mut reg = Registry::default();
        reg.upsert(Installed {
            id: "hello".into(),
            version: "0.1.0".into(),
            on: true,
            source: Source::Dev {
                path: "/tmp/dev dir/hello".into(),
            },
        });
        reg.upsert(Installed {
            id: "claude-sessions".into(),
            version: "0.1.0".into(),
            on: false,
            source: Source::Catalog,
        });
        reg.upsert(Installed {
            id: "hello".into(),
            version: "0.2.0".into(),
            on: true,
            source: Source::Dev {
                path: "/tmp/dev dir/hello".into(),
            },
        });
        reg.set_on("claude-sessions", true);
        reg.save(&root).map_err(|e| e.to_string())?;

        let back = Registry::load(&root).map_err(|e| e.to_string())?;
        assert_eq!(back, reg);
        assert_eq!(back.extensions[0].version, "0.2.0");
        assert!(back.get("claude-sessions").is_some_and(|i| i.on));
        assert!(!root.path().join(".installed.toml.tmp").exists());
        Ok(())
    }
}
