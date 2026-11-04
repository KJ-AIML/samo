//! Samo's extension host (L5, ADR-002): extensions on disk, and the helpers they run.
//!
//! Everything lives under one folder (`~/Library/Application Support/Samo/Extensions/`, or `SAMO_EXT_ROOT`):
//! `installed.toml` says what is installed and whether it is on, `history.log` records every change, and each
//! extension has `<id>/<version>/`, a `current` link to the version in use, and its own `data/`.

mod error;
mod family;
mod history;
mod host;
mod install;
mod limits;
mod manage;
mod meter;
pub mod net;
mod paths;
mod registry;
mod state;
pub mod store;
mod supervisor;
mod tree;
pub mod update;
pub mod windows;
pub mod worker;

pub use error::InstallError;
pub use history::History;
pub use host::{Host, HostConfig, MAX_EXTENSIONS, this_macos};
pub use install::{DevInstall, find_dev_packages, install_dev};
pub use manage::{StoreInstall, UninstallFailed};
pub use paths::ExtRoot;
pub use registry::{Installed, Registry};
pub use samo_domain::extensions::Source;
pub use samo_ext_protocol::PopupKind;
pub use state::{ExtSlot, ExtSnapshot, ExtView, PendingPopup, Status};
pub use supervisor::MAX_ROWS;

/// Whether the extension store runs on this OS: the Mac and Windows (#10, ADR-006).
pub const STORE_ON_THIS_OS: bool = cfg!(any(target_os = "macos", windows));

/// Whether Samo updates itself from the store on this OS. On Windows that comes with Samo's own zip; until then
/// Settings says so and nothing is checked (ADR-006 R28).
pub const SAMO_UPDATES_ON_THIS_OS: bool = cfg!(target_os = "macos");
