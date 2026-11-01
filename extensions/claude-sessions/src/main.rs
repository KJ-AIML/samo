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
        "statusline" => {
            samo_claude::statusline::run();
            ExitCode::SUCCESS
        }
        "serve" => samo_claude::serve::run(),
        "on" | "off" | "uninstall" => {
            let (Some(home), Some(data)) =
                (samo_claude::paths::home(), samo_claude::paths::data_dir())
            else {
                eprintln!("samo-claude: no home folder");
                return ExitCode::FAILURE;
            };
            let places = samo_claude::connection::Places { home, data };
            let done = match verb.as_str() {
                "on" => samo_claude::connection::on(&places),
                "off" => samo_claude::connection::off(&places),
                _ => samo_claude::connection::uninstall(&places),
            };
            match done {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("samo-claude {verb}: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("usage: samo-claude hook|statusline|serve|on|off|uninstall");
            ExitCode::from(2)
        }
    }
}
