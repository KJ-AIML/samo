use samo_ext_protocol::{Change, PopupKind, Widget};
use std::sync::Arc;

/// Which installed extension, by its place in the list; stable while Samo runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtSlot(pub u8);

/// What an extension is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Its helper is starting, or restarting after a crash.
    Starting,
    Running,
    /// Turned off by the user.
    Off,
    /// It stopped and won't restart by itself; the reason says why, and Restart tries again.
    Stopped(String),
    /// It can't run here (a dev build without `SAMO_EXT_DEV_DIR`, or it needs a newer Samo).
    Blocked(String),
}

/// One extension, as the UI draws it.
#[derive(Debug, Clone)]
pub struct ExtView {
    pub slot: ExtSlot,
    pub id: String,
    pub name: String,
    pub summary: String,
    pub version: String,
    pub dev: bool,
    pub on: bool,
    pub status: Status,
    /// What the helper last said about its health, or why switching it failed.
    pub note: Option<String>,
    pub changes: Vec<Change>,
    /// The last page the helper sent.
    pub view: Arc<Vec<Widget>>,
    /// How many things wait for the user, and the quick panel's line.
    pub badge: u32,
    pub badge_summary: String,
}

/// Every installed extension, copied out for one render.
#[derive(Debug, Clone, Default)]
pub struct ExtSnapshot {
    pub extensions: Vec<ExtView>,
    /// Things that went wrong loading extensions (a damaged install, a dev folder that won't install).
    pub problems: Vec<String>,
}

impl ExtSnapshot {
    pub fn get(&self, slot: ExtSlot) -> Option<&ExtView> {
        self.extensions.iter().find(|e| e.slot == slot)
    }

    /// Everything waiting for the user, across extensions that are on.
    pub fn attention(&self) -> u32 {
        self.extensions
            .iter()
            .filter(|e| e.on)
            .map(|e| e.badge)
            .fold(0, u32::saturating_add)
    }
}

/// A pop-up an extension asked for, already through the host's limits (R8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPopup {
    pub slot: ExtSlot,
    pub key: String,
    pub kind: PopupKind,
    pub title: String,
    pub body: String,
}
