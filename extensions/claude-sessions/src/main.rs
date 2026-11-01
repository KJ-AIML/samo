//! `samo-claude`: the Claude Sessions extension's helper.
//!
//! - `hook`: Claude Code runs it on each session event.
//! - `statusline`: Claude Code runs it for its status line, when Plan limits is on.
//! - `serve`: Samo runs it while the extension is on.
//! - `on`, `off`, `uninstall`: Samo runs them to restore or undo what it changed in Claude Code's settings.

use std::process::ExitCode;

fn main() -> ExitCode {
    let verb = std::env::args().nth(1).unwrap_or_default();
    match verb.as_str() {
        "hook" => {
            samo_claude::hook::run();
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: samo-claude hook|statusline|serve|on|off|uninstall");
            ExitCode::from(2)
        }
    }
}
