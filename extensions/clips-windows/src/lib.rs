//! Clips for Windows: its core, ported from the Mac's ClipsCore with its tests (ADR-005), pure and tested on every
//! OS (ADR-006 R25).

pub mod clips;
pub mod dib;
#[cfg(test)]
mod fake;
pub mod filters;
pub mod history;
pub mod page;
pub mod paste;
pub mod search;
pub mod serve;
pub mod settings;
pub mod store;
pub mod watcher;
#[cfg(windows)]
pub mod win;
