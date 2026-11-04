//! Which process is Claude Code: the hook's nearest ancestor that isn't a shell. Claude Code may run a hook
//! through a shell, and the shell may run another, so the shells are skipped.

use samo_primitives::ProcessIdentity;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// One process in the chain from the hook upward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub start_time_secs: u64,
    pub name: String,
}

/// Shells Claude Code runs a hook or status line through: `sh -c` on the Mac; on Windows Git Bash (two
/// `bash.exe`), or PowerShell when Git Bash is missing (ADR-006).
const SHELLS: [&str; 9] = [
    "sh",
    "bash",
    "zsh",
    "dash",
    "fish",
    "ksh",
    "cmd",
    "pwsh",
    "powershell",
];

fn is_shell(name: &str) -> bool {
    // A login shell's name starts with `-`; a path may come with it. Windows names end in `.exe`, in any case.
    let base = name
        .trim_start_matches('-')
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name);
    let base = base.to_ascii_lowercase();
    let base = base.strip_suffix(".exe").unwrap_or(&base);
    SHELLS.contains(&base)
}

/// On Windows, `chain` (parent first) up to the first process that started after the one below it: that one isn't
/// its parent but a later process given a reused pid (Windows keeps an orphan's old parent pid). `own_start` is
/// this process's start time. The Mac gives an orphan to launchd, so its chain is kept as it is (ADR-006 R25).
fn until_reused_on(chain: Vec<ProcInfo>, own_start: u64, windows: bool) -> Vec<ProcInfo> {
    if !windows {
        return chain;
    }
    let mut below = own_start;
    chain
        .into_iter()
        .take_while(|p| {
            let in_order = p.start_time_secs <= below;
            below = p.start_time_secs;
            in_order
        })
        .collect()
}

/// Claude Code's identity, given the hook's ancestors (parent first).
pub fn claude_identity(chain: &[ProcInfo]) -> Option<ProcessIdentity> {
    chain
        .iter()
        .find(|p| !is_shell(&p.name) && p.pid > 1)
        .map(|p| ProcessIdentity {
            pid: p.pid,
            start_time_secs: p.start_time_secs,
        })
}

/// This process's ancestors, parent first, at most `depth` of them.
pub fn ancestors(depth: usize) -> Vec<ProcInfo> {
    let mut sys = System::new();
    let mut chain = Vec::new();
    let mut pid = Pid::from_u32(std::process::id());
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        false,
        ProcessRefreshKind::nothing(),
    );
    let own_start = sys
        .process(pid)
        .map_or(u64::MAX, sysinfo::Process::start_time);
    for _ in 0..=depth {
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            false,
            ProcessRefreshKind::nothing(),
        );
        let Some(parent) = sys.process(pid).and_then(sysinfo::Process::parent) else {
            break;
        };
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[parent]),
            false,
            ProcessRefreshKind::nothing(),
        );
        let Some(process) = sys.process(parent) else {
            break;
        };
        chain.push(ProcInfo {
            pid: parent.as_u32(),
            start_time_secs: process.start_time(),
            name: process.name().to_string_lossy().into_owned(),
        });
        if chain.len() >= depth {
            break;
        }
        pid = parent;
    }
    until_reused_on(chain, own_start, cfg!(windows))
}

/// Whether each process is still the one recorded (same pid and start time).
pub fn alive(identities: &[ProcessIdentity]) -> Vec<bool> {
    if identities.is_empty() {
        return Vec::new();
    }
    let pids: Vec<Pid> = identities.iter().map(|i| Pid::from_u32(i.pid)).collect();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing(),
    );
    identities
        .iter()
        .map(|i| {
            sys.process(Pid::from_u32(i.pid))
                .is_some_and(|p| p.start_time() == i.start_time_secs)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, name: &str) -> ProcInfo {
        ProcInfo {
            pid,
            start_time_secs: u64::from(pid) * 10,
            name: name.into(),
        }
    }

    #[test]
    fn nearest_non_shell_ancestor_is_claude() {
        let chain = [
            p(300, "sh"),
            p(200, "-zsh"),
            p(100, "claude"),
            p(1, "launchd"),
        ];
        assert_eq!(
            claude_identity(&chain),
            Some(ProcessIdentity {
                pid: 100,
                start_time_secs: 1_000
            })
        );
        // Run directly, the parent is Claude Code.
        assert_eq!(claude_identity(&[p(100, "node")]).map(|i| i.pid), Some(100));
        assert_eq!(claude_identity(&[p(300, "/bin/bash")]), None);
        assert_eq!(claude_identity(&[p(1, "launchd")]), None);
    }

    #[test]
    fn windows_shells_are_skipped() {
        // Git Bash runs a hook through two bash.exe, PowerShell through pwsh.exe or powershell.exe.
        let chain = [
            p(500, "bash.exe"),
            p(400, "BASH.EXE"),
            p(300, "pwsh.exe"),
            p(250, "powershell.exe"),
            p(200, "cmd.exe"),
            p(100, "claude.exe"),
        ];
        assert_eq!(claude_identity(&chain).map(|i| i.pid), Some(100));
        assert_eq!(
            claude_identity(&[p(300, r"C:\Program Files\Git\usr\bin\bash.exe")]),
            None
        );
    }

    #[test]
    fn a_parent_started_later_is_not_the_parent() {
        // Windows keeps an orphan's old parent pid, and gives pids out again: a "parent" that started after its
        // child is some other process, and the chain stops before it.
        let own_start = 1_000;
        let chain = vec![
            ProcInfo {
                pid: 20,
                start_time_secs: 900,
                name: "bash.exe".into(),
            },
            ProcInfo {
                pid: 10,
                start_time_secs: 950,
                name: "explorer.exe".into(),
            },
        ];
        let kept = until_reused_on(chain.clone(), own_start, true);
        assert_eq!(kept.iter().map(|c| c.pid).collect::<Vec<_>>(), [20]);
        // The Mac gives an orphan to launchd, so its chain is as it was, whatever the clock did (ADR-006 R25).
        assert_eq!(until_reused_on(chain.clone(), own_start, false), chain);
        // In order, every one is kept.
        let mut fine = chain;
        fine[1].start_time_secs = 900;
        assert_eq!(until_reused_on(fine, own_start, true).len(), 2);
        assert!(until_reused_on(vec![p(30, "claude.exe")], 10, true).is_empty());
    }

    #[test]
    fn this_process_has_a_living_parent() {
        let chain = ancestors(3);
        assert!(!chain.is_empty());
        let ids: Vec<ProcessIdentity> = chain
            .iter()
            .map(|c| ProcessIdentity {
                pid: c.pid,
                start_time_secs: c.start_time_secs,
            })
            .collect();
        assert!(alive(&ids).iter().all(|a| *a));
        let gone = ProcessIdentity {
            pid: chain[0].pid,
            start_time_secs: chain[0].start_time_secs + 1,
        };
        assert_eq!(alive(&[gone]), vec![false]);
    }
}
