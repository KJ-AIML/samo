//! A helper's family: the helper and every process it started, ended together (ADR-004 R15, R17).
//!
//! On the Mac the family is found by walking parent pids (`tree`). On Windows the helper goes into a Job Object
//! closed with kill-on-close, and the family is the job's own list: every process the helper started, whatever its
//! parent pid says now, and nothing else. It ends when the helper stops, and when Samo does, however Samo ends. A
//! helper that couldn't be put in a job falls back to the walk (ADR-006).

use crate::tree::{self, Member};
use std::process::Child;
use sysinfo::System;

#[derive(Default)]
pub struct Family {
    root: u32,
    members: Vec<Member>,
    #[cfg(windows)]
    job: Option<win32job::Job>,
}

impl Family {
    /// Takes `child` (a helper just started) as the family's root. On Windows it goes into a new Job Object first;
    /// `Err` says why it couldn't, and the family is then found by walking.
    pub fn adopt(&mut self, child: &Child, sys: &mut System) -> Result<(), String> {
        self.root = child.id();
        let contained = self.contain(child);
        self.walk(sys);
        contained
    }

    #[cfg(windows)]
    fn contain(&mut self, child: &Child) -> Result<(), String> {
        use std::os::windows::io::AsRawHandle;
        let mut limits = win32job::ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let job = win32job::Job::create_with_limit_info(&limits).map_err(|e| e.to_string())?;
        // A handle is a pointer-sized value; the job only reads it.
        job.assign_process(child.as_raw_handle() as isize)
            .map_err(|e| e.to_string())?;
        self.job = Some(job);
        Ok(())
    }

    #[cfg(not(windows))]
    fn contain(&mut self, _child: &Child) -> Result<(), String> {
        Ok(())
    }

    /// The family as it is now: what has exited is dropped.
    pub fn walk(&mut self, sys: &mut System) {
        self.members = self.now(sys);
    }

    /// Adds the family as it is now to what was known: a child started just before a stop is caught, and one
    /// already orphaned is kept.
    pub fn refresh(&mut self, sys: &mut System) {
        for m in self.now(sys) {
            if !self.members.contains(&m) {
                self.members.push(m);
            }
        }
    }

    /// Who is in the family now: on Windows the job's list, else (or without a job) the walk.
    fn now(&self, sys: &mut System) -> Vec<Member> {
        #[cfg(windows)]
        if let Some(pids) = self
            .job
            .as_ref()
            .and_then(|job| job.query_process_id_list().ok())
        {
            let pids: Vec<u32> = pids
                .into_iter()
                .filter_map(|p| u32::try_from(p).ok())
                .collect();
            return job_family(self.root, &pids, &tree::procs(sys));
        }
        tree::tree(sys, self.root)
    }

    /// Who is in it, parents before children, as last walked.
    #[cfg(test)]
    pub fn members(&self) -> &[Member] {
        &self.members
    }

    pub fn pids(&self) -> Vec<u32> {
        self.members.iter().map(|m| m.pid).collect()
    }

    /// Ends every member that is still the process it was, children first; on Windows, closing the job then ends
    /// anything left in it.
    pub fn end(&mut self, sys: &mut System) {
        tree::kill(sys, &self.members);
        #[cfg(windows)]
        drop(self.job.take());
    }
}

/// The family in a job: the processes in `job` that are still running (in `procs`), the helper (`root`) first,
/// then by start time, so parents come before their children. Parent pids aren't read at all: in a job they can't
/// mistake another process for the helper's.
#[cfg(any(windows, test))]
fn job_family(root: u32, job: &[u32], procs: &[tree::Proc]) -> Vec<Member> {
    let mut members: Vec<Member> = procs
        .iter()
        .filter(|p| job.contains(&p.pid))
        .map(|p| Member {
            pid: p.pid,
            start: p.start,
        })
        .collect();
    members.sort_by_key(|m| (m.pid != root, m.start, m.pid));
    members
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate};

    /// A process that starts one child and waits.
    fn parent_of_a_sleeper() -> Result<Child, String> {
        #[cfg(windows)]
        let mut command = {
            let root = std::env::var("SystemRoot").map_err(|e| e.to_string())?;
            let mut c = Command::new(format!(r"{root}\System32\cmd.exe"));
            c.args([
                "/c",
                &format!(r"{root}\System32\PING.EXE"),
                "-n",
                "30",
                "127.0.0.1",
            ]);
            c
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut c = Command::new("/bin/sh");
            c.args(["-c", "sleep 30 & wait"]);
            c
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())
    }

    fn running(sys: &mut System, members: &[Member]) -> Vec<u32> {
        let pids: Vec<Pid> = members.iter().map(|m| Pid::from_u32(m.pid)).collect();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids),
            true,
            ProcessRefreshKind::nothing(),
        );
        members
            .iter()
            .filter(|m| {
                sys.process(Pid::from_u32(m.pid))
                    .is_some_and(|p| p.start_time() == m.start)
            })
            .map(|m| m.pid)
            .collect()
    }

    fn proc(pid: u32, parent: u32, start: u64) -> tree::Proc {
        tree::Proc {
            pid,
            parent: Some(parent),
            start,
        }
    }

    #[test]
    fn with_a_job_the_family_is_the_job() {
        let procs = [
            proc(10, 1, 100),
            proc(11, 10, 100),
            // Names the helper as its parent and started in the same second, but isn't in its job: a process the
            // pid's previous owner started. It is neither ended nor window-checked.
            proc(12, 10, 100),
            // An orphan whose parent is gone, still in the job.
            proc(14, 99, 102),
            proc(13, 11, 101),
        ];
        let members = job_family(10, &[14, 10, 11, 13, 77], &procs);
        let pids: Vec<u32> = members.iter().map(|m| m.pid).collect();
        // The helper first, then by start time, so parents come before their children; a pid that has gone is
        // left out.
        assert_eq!(pids, [10, 11, 13, 14]);
        // The helper gone, what it started is still its family until the job ends it.
        let left: Vec<u32> = job_family(10, &[11], &procs[1..])
            .iter()
            .map(|m| m.pid)
            .collect();
        assert_eq!(left, [11]);
    }

    #[test]
    fn ending_a_family_ends_its_children() -> Result<(), String> {
        let mut child = parent_of_a_sleeper()?;
        let mut sys = System::new();
        let mut family = Family::default();
        family.adopt(&child, &mut sys)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while family.members().len() < 2 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
            family.walk(&mut sys);
        }
        let members = family.members().to_vec();
        assert!(members.len() >= 2, "the child wasn't found: {members:?}");
        assert_eq!(members.first().map(|m| m.pid), Some(child.id()));

        family.end(&mut sys);
        let _ = child.wait();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut left = running(&mut sys, &members);
        while !left.is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
            left = running(&mut sys, &members);
        }
        assert!(left.is_empty(), "still running: {left:?}");
        Ok(())
    }

    /// A child whose parent has already exited can't be found by parent; on Windows its job still holds it.
    #[cfg(windows)]
    #[test]
    fn an_orphan_still_ends_with_the_family() -> Result<(), String> {
        let root = std::env::var("SystemRoot").map_err(|e| e.to_string())?;
        let mut child = Command::new(format!(r"{root}\System32\cmd.exe"))
            .args(["/c", "start", "", "/b"])
            .arg(format!(r"{root}\System32\PING.EXE"))
            .args(["-n", "30", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut sys = System::new();
        let mut family = Family::default();
        family.adopt(&child, &mut sys)?;
        // cmd starts ping and exits at once, leaving ping an orphan.
        let _ = child.wait();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut orphans = Vec::new();
        while orphans.is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
            family.walk(&mut sys);
            orphans = family.members().to_vec();
        }
        assert_eq!(orphans.len(), 1, "the orphan wasn't found: {orphans:?}");

        family.end(&mut sys);
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut left = running(&mut sys, &orphans);
        while !left.is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
            left = running(&mut sys, &orphans);
        }
        assert!(left.is_empty(), "still running: {left:?}");
        Ok(())
    }
}
