//! L5 on Windows (ADR-006): the scanner, the Recycle Bin mover, Quit and telemetry, with the same functions as
//! `samo-macos`. Each app takes the platform crate of the OS it is built for; on any other OS this crate is empty.
#![cfg(windows)]

pub mod cleanup;
pub mod identity;
pub mod scanner;

pub use cleanup::{TrashOutcome, default_user_trash_dir, move_to_trash, restore_from_trash};
pub use identity::inspect_node_identity;
pub use scanner::scan_directory;
