//! Connecting Claude Sessions to Claude Code and back (ADR-002 C1, C3, R7): the file side of `settings.rs`.
//! What Samo created is recorded in `data/connect.json`; what was active when the extension was turned off is
//! recorded in `data/was.json`, so turning it on restores exactly that.

use crate::settings::{self, Created, Refused};
use crate::settings_io::{backup_once, read, remove, settings_path, write_if_unchanged};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Where things are: the home folder (for `~/.claude`) and the extension's data folder.
#[derive(Debug, Clone)]
pub struct Places {
    pub home: PathBuf,
    pub data: PathBuf,
}

impl Places {
    pub fn settings(&self) -> PathBuf {
        settings_path(&self.home)
    }
    fn created_file(&self) -> PathBuf {
        self.data.join("connect.json")
    }
    fn was_file(&self) -> PathBuf {
        self.data.join("was.json")
    }
}

/// What is set in Claude Code's settings right now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Active {
    pub connected: bool,
    /// Samo's status line is set.
    pub plan_limits: bool,
    /// A status line that isn't Samo's is set.
    #[serde(skip)]
    pub other_statusline: bool,
}

pub fn active(places: &Places) -> Result<Active, String> {
    let (v, _) = read(&places.settings())?;
    Ok(Active {
        connected: settings::is_connected(&v),
        plan_limits: settings::statusline_owner(&v) == Some(true),
        other_statusline: settings::statusline_owner(&v) == Some(false),
    })
}

/// Reads, changes and writes the settings, trying again if Claude Code wrote them in between.
fn edit(
    path: &Path,
    mut change: impl FnMut(Value, bool) -> Result<Option<Value>, String>,
) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..3 {
        let (value, seen) = read(path)?;
        let Some(next) = change(value, seen.existed())? else {
            return Ok(());
        };
        match write_if_unchanged(path, &next, &seen) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn load_created(places: &Places) -> Option<Created> {
    fs::read(places.created_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

pub fn connect(places: &Places) -> Result<(), String> {
    let path = places.settings();
    backup_once(&path)?;
    let mut made = None;
    edit(&path, |v, existed| {
        if settings::is_connected(&v) {
            return Ok(None);
        }
        let (next, created) = settings::connect(v, existed).map_err(|e: Refused| e.to_string())?;
        made = Some(created);
        Ok(Some(next))
    })?;
    if let Some(created) = made {
        fs::create_dir_all(&places.data).map_err(|e| e.to_string())?;
        let text = serde_json::to_vec(&created).map_err(|e| e.to_string())?;
        fs::write(places.created_file(), text).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn disconnect(places: &Places) -> Result<(), String> {
    let path = places.settings();
    let created = load_created(places);
    let mut delete = false;
    edit(&path, |v, _| {
        if !settings::is_connected(&v) {
            return Ok(None);
        }
        let (next, del) = settings::disconnect(v, created.as_ref());
        delete = del;
        Ok(Some(next))
    })?;
    if delete {
        remove(&path)?;
    }
    let _ = fs::remove_file(places.created_file());
    Ok(())
}

pub fn plan_limits_on(places: &Places) -> Result<(), String> {
    let path = places.settings();
    backup_once(&path)?;
    edit(&path, |v, _| {
        if settings::statusline_owner(&v) == Some(true) {
            return Ok(None);
        }
        settings::set_statusline(v)
            .map(Some)
            .map_err(|e| e.to_string())
    })
}

pub fn plan_limits_off(places: &Places) -> Result<(), String> {
    edit(&places.settings(), |v, _| {
        if settings::statusline_owner(&v) != Some(true) {
            return Ok(None);
        }
        Ok(Some(settings::clear_statusline(v)))
    })
}

/// `samo-claude off`: undoes what is active and remembers it for `on`.
pub fn off(places: &Places) -> Result<(), String> {
    let now = active(places)?;
    if now.connected {
        disconnect(places)?;
    }
    if now.plan_limits {
        plan_limits_off(places)?;
    }
    fs::create_dir_all(&places.data).map_err(|e| e.to_string())?;
    let text = serde_json::to_vec(&now).map_err(|e| e.to_string())?;
    fs::write(places.was_file(), text).map_err(|e| e.to_string())
}

/// `samo-claude on`: restores what `off` undid.
pub fn on(places: &Places) -> Result<(), String> {
    let Some(was) = fs::read(places.was_file())
        .ok()
        .and_then(|b| serde_json::from_slice::<Active>(&b).ok())
    else {
        return Ok(());
    };
    if was.connected {
        connect(places)?;
    }
    if was.plan_limits {
        // Someone may have set their own status line meanwhile; Samo never replaces it.
        let _ = plan_limits_on(places);
    }
    let _ = fs::remove_file(places.was_file());
    Ok(())
}

/// `samo-claude uninstall`: undoes everything, for good.
pub fn uninstall(places: &Places) -> Result<(), String> {
    disconnect(places)?;
    plan_limits_off(places)?;
    let _ = fs::remove_file(places.was_file());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places() -> Result<(tempfile::TempDir, Places), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let places = Places {
            home: dir.path().join("home"),
            data: dir.path().join("data"),
        };
        fs::create_dir_all(places.home.join(".claude")).map_err(|e| e.to_string())?;
        Ok((dir, places))
    }

    #[test]
    fn off_then_on_restores_hooks_and_statusline() -> Result<(), String> {
        let (_dir, p) = places()?;
        fs::write(p.settings(), "{\n  \"model\": \"opus\"\n}\n").map_err(|e| e.to_string())?;
        connect(&p)?;
        plan_limits_on(&p)?;
        assert_eq!(
            active(&p)?,
            Active {
                connected: true,
                plan_limits: true,
                other_statusline: false
            }
        );

        off(&p)?;
        let (v, _) = read(&p.settings())?;
        assert_eq!(v, serde_json::json!({"model": "opus"}));

        on(&p)?;
        let now = active(&p)?;
        assert!(now.connected && now.plan_limits);

        uninstall(&p)?;
        let (v, _) = read(&p.settings())?;
        assert_eq!(v, serde_json::json!({"model": "opus"}));
        Ok(())
    }

    #[test]
    fn plan_limits_switch_refuses_foreign_statusline() -> Result<(), String> {
        let (_dir, p) = places()?;
        fs::write(
            p.settings(),
            r#"{"statusLine":{"type":"command","command":"~/bin/mine.sh"}}"#,
        )
        .map_err(|e| e.to_string())?;
        assert!(plan_limits_on(&p).is_err());
        assert!(active(&p)?.other_statusline);
        // Turning off never removes it either.
        plan_limits_off(&p)?;
        assert!(active(&p)?.other_statusline);
        Ok(())
    }

    #[test]
    fn connect_on_a_missing_file_and_back_leaves_no_file() -> Result<(), String> {
        let (_dir, p) = places()?;
        connect(&p)?;
        assert!(p.settings().is_file());
        assert!(
            !p.settings()
                .with_file_name("settings.json.samo-bak")
                .exists()
        );
        disconnect(&p)?;
        assert!(!p.settings().exists());
        Ok(())
    }
}
