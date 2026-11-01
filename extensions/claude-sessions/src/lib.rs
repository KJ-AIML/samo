//! Claude Sessions (ADR-002 C1–C3): Claude Code's sessions in Samo, as an extension.
//!
//! Claude Code runs `samo-claude hook` on each session event and `samo-claude statusline` for its status line;
//! both write a small file into the extension's data folder and exit at once. Samo runs `samo-claude serve`,
//! which watches those files and the day's transcripts and sends Samo the page, the badge and pop-ups.
//!
//! Everything here is built on Claude Code's documented hooks and status line. It never goes online.

pub mod ancestry;
pub mod hook;
pub mod paths;
pub mod sessions;
pub mod settings;
pub mod settings_io;
pub mod transcripts;
pub mod usage;
