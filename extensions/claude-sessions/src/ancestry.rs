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

const SHELLS: [&str; 6] = ["sh", "bash", "zsh", "dash", "fish", "ksh"];

fn is_shell(name: &str) -> bool {
    // A login shell's name starts with `-`; a path may come with it.
    let base = name
        .trim_start_matches('-')
        .rsplit('/')
        .next()
        .unwrap_or(name);
    SHELLS.contains(&base)
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
    chain
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
