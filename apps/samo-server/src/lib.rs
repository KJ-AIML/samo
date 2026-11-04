pub mod http;

// ADR-006: the platform crate of the OS this is built for, with the same functions on each.
#[cfg(target_os = "macos")]
pub(crate) use samo_macos as platform;
#[cfg(windows)]
pub(crate) use samo_windows as platform;

pub use http::{AppState, build_router, initialize_server_state};
