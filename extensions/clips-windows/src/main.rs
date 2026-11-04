//! `samo-clips`: Clips' helper on Windows (ADR-005, ADR-006). Samo runs `samo-clips serve` while Clips is on; `on`,
//! `off` and `uninstall` have nothing outside Clips' folder to undo (Samo deletes the folder on uninstall).

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("serve") => serve(),
        Some("on" | "off" | "uninstall") => ExitCode::SUCCESS,
        _ => {
            eprintln!("usage: samo-clips serve|on|off|uninstall");
            ExitCode::from(2)
        }
    }
}

#[cfg(windows)]
fn serve() -> ExitCode {
    use samo_clips::win::board::{WinBoard, front_app};
    samo_clips::serve::run(
        WinBoard::default(),
        Box::new(front_app),
        &samo_clips::win::WinDesk,
    )
}

#[cfg(not(windows))]
fn serve() -> ExitCode {
    eprintln!("samo-clips: this is Clips for Windows; the Mac's is extensions/clips");
    ExitCode::FAILURE
}
