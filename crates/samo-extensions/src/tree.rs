//! A helper's process tree: the helper and everything it started (ADR-004, R15, R17). Each member is known by its
//! pid *and* start time (R4's identity), so a pid the system has reused is never mistaken for one of them.

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub pid: u32,
    pub start: u64,
}

/// One process as the system lists it: who it says its parent is, and when it started (seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub parent: Option<u32>,
    pub start: u64,
}

/// `root` and its descendants, parents before children. Empty if `root` is gone.
pub fn tree(sys: &mut System, root: u32) -> Vec<Member> {
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let procs: Vec<Proc> = sys
        .processes()
        .iter()
        .map(|(pid, p)| Proc {
            pid: pid.as_u32(),
            parent: p.parent().map(Pid::as_u32),
            start: p.start_time(),
        })
        .collect();
    descendants(&procs, root)
}

/// `root` and its descendants in `procs`, parents before children. A process is a child only if it started no
/// earlier than its parent: Windows keeps an orphan's old parent pid, and gives pids out again quickly, so an
/// unrelated process can name the helper's pid as its parent (ADR-006).
pub fn descendants(procs: &[Proc], root: u32) -> Vec<Member> {
    let Some(top) = procs.iter().find(|p| p.pid == root) else {
        return Vec::new();
    };
    let mut members = vec![Member {
        pid: root,
        start: top.start,
    }];
    let mut next = 0;
    while let Some(parent) = members.get(next).copied() {
        next += 1;
        let children: Vec<Member> = procs
            .iter()
            .filter(|p| p.parent == Some(parent.pid) && p.start >= parent.start)
            .map(|p| Member {
                pid: p.pid,
                start: p.start,
            })
            .filter(|m| !members.contains(m))
            .collect();
        members.extend(children);
        // A process tree this large isn't a helper's; stop looking rather than walk it.
        if members.len() > 256 {
            break;
        }
    }
    members
}

/// Adds `root`'s tree as it is now to `known`, keeping what was known before: a child started just before a stop
/// is caught, and one already orphaned is kept.
pub fn refresh(sys: &mut System, root: u32, known: &mut Vec<Member>) {
    for m in tree(sys, root) {
        if !known.contains(&m) {
            known.push(m);
        }
    }
}

/// Kills every member that is still the process it was, children first.
pub fn kill(sys: &mut System, members: &[Member]) {
    let pids: Vec<Pid> = members.iter().map(|m| Pid::from_u32(m.pid)).collect();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing(),
    );
    for m in members.iter().rev() {
        if let Some(p) = sys.process(Pid::from_u32(m.pid))
            && p.start_time() == m.start
        {
            p.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: u32, start: u64) -> Proc {
        Proc {
            pid,
            parent: Some(parent),
            start,
        }
    }

    #[test]
    fn a_child_never_started_before_its_parent() {
        let procs = [
            proc(10, 1, 100),
            proc(11, 10, 101),
            proc(12, 11, 105),
            // An orphan whose parent's pid 10 Windows has since given the helper: it started first, so it isn't
            // the helper's, and neither is what it started.
            proc(13, 10, 50),
            proc(14, 13, 120),
            // Started in the same second as the helper (start times are in seconds).
            proc(15, 10, 100),
        ];
        let pids: Vec<u32> = descendants(&procs, 10).iter().map(|m| m.pid).collect();
        assert_eq!(pids, [10, 11, 15, 12]);
        assert!(descendants(&procs, 99).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn tree_finds_children_and_kills_them() -> Result<(), String> {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut sys = System::new();
        let members = tree(&mut sys, std::process::id());
        let found = members.iter().any(|m| m.pid == child.id());
        // Only this test's own child: other tests share this process.
        let mine: Vec<Member> = members
            .iter()
            .filter(|m| m.pid == child.id())
            .copied()
            .collect();
        kill(&mut sys, &mine);
        let ended = child.wait().map_err(|e| e.to_string())?;
        assert!(found, "{} not in {members:?}", child.id());
        assert!(!ended.success(), "it was killed");
        Ok(())
    }
}
