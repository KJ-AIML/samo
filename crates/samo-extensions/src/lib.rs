//! Samo's extension host (L5, ADR-002): extensions on disk, and the helpers they run.
//!
//! Everything lives under one folder (`~/Library/Application Support/Samo/Extensions/`, or `SAMO_EXT_ROOT`):
//! `installed.toml` says what is installed and whether it is on, `history.log` records every change, and each
//! extension has `<id>/<version>/`, a `current` link to the version in use, and its own `data/`.

mod error;
mod history;
mod host;
mod install;
mod limits;
pub mod net;
mod paths;
mod registry;
mod state;
pub mod store;
mod supervisor;

pub use error::InstallError;
pub use history::History;
pub use host::{Host, HostConfig, MAX_EXTENSIONS};
pub use install::{DevInstall, find_dev_packages, install_dev};
pub use paths::ExtRoot;
pub use registry::{Installed, Registry};
pub use samo_domain::extensions::Source;
pub use samo_ext_protocol::PopupKind;
pub use state::{ExtSlot, ExtSnapshot, ExtView, PendingPopup, Status};
pub use supervisor::MAX_ROWS;
