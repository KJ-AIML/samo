//! A helper's process tree: the helper and everything it started (ADR-004, R15, R17). Each member is known by its
//! pid *and* start time (R4's identity), so a pid the system has reused is never mistaken for one of them.

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub pid: u32,
    pub start: u64,
}

/// `root` and its descendants, parents before children. Empty if `root` is gone.
pub fn tree(sys: &mut System, root: u32) -> Vec<Member> {
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let Some(top) = sys.process(Pid::from_u32(root)) else {
        return Vec::new();
    };
    let mut members = vec![Member {
        pid: root,
        start: top.start_time(),
    }];
    let mut next = 0;
    while let Some(parent) = members.get(next).map(|m| m.pid) {
        next += 1;
        let parent = Pid::from_u32(parent);
        let children: Vec<Member> = sys
            .processes()
            .iter()
            .filter(|(_, p)| p.parent() == Some(parent))
            .map(|(pid, p)| Member {
                pid: pid.as_u32(),
                start: p.start_time(),
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

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
