//! Samo's extension host (L5, ADR-002): extensions on disk, and the helpers they run.
//!
//! Everything lives under one folder (`~/Library/Application Support/Samo/Extensions/`, or `SAMO_EXT_ROOT`):
//! `installed.toml` says what is installed and whether it is on, `history.log` records every change, and each
//! extension has `<id>/<version>/`, a `current` link to the version in use, and its own `data/`.

mod error;
mod history;
mod install;
mod paths;
mod registry;

pub use error::InstallError;
pub use history::History;
pub use install::{DevInstall, find_dev_packages, install_dev};
pub use paths::ExtRoot;
pub use registry::{Installed, Registry};
pub use samo_domain::extensions::Source;
