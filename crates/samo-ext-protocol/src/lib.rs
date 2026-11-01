//! What Samo and an extension's helper agree on: the package manifest, and protocol v1's messages and widgets.
//!
//! Pure: no files, no processes, no network. Both Samo and every helper depend on this crate, so a helper never
//! needs Samo's internals.

mod manifest;
mod message;
mod widget;

pub use manifest::{Change, Manifest, SettingDecl, valid_id, valid_version};
pub use message::{
    BadgeMeter, HelperMsg, HostMsg, PopupKind, decode_helper_line, decode_host_line, encode,
};
pub use widget::{ButtonKind, Lamp, NoteKind, RowItem, Widget, rows_in};

/// The protocol version this crate speaks. A helper says it in `hello`; a manifest names it.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("not a protocol v1 message: {0}")]
    Message(String),
    #[error("extension.toml is not valid: {0}")]
    Manifest(String),
}
