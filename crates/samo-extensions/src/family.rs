//! A helper's family: the helper and every process it started, ended together (ADR-004 R15, R17).
//!
//! Members are found by walking parent pids (`tree`). On Windows they are also kept in a Job Object closed with
//! kill-on-close, so whatever the walk can't see (a child whose parent already exited) still ends with the helper,
//! and the whole family ends when Samo does, however it ends (ADR-006).

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
    /// `Err` says why it couldn't, and the family is then found by walking alone.
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
        self.members = tree::tree(sys, self.root);
        self.add_job_members(sys);
    }

    /// Adds the family as it is now to what was known: a child started just before a stop is caught, and one
    /// already orphaned is kept.
    pub fn refresh(&mut self, sys: &mut System) {
        tree::refresh(sys, self.root, &mut self.members);
        self.add_job_members(sys);
    }

    /// On Windows, the job's own list: every process in it, whoever its parent was. `sys` was just refreshed by
    /// the walk, so each one's start time is there.
    #[cfg(windows)]
    fn add_job_members(&mut self, sys: &System) {
        let Some(pids) = self
            .job
            .as_ref()
            .and_then(|job| job.query_process_id_list().ok())
        else {
            return;
        };
        for pid in pids.into_iter().filter_map(|p| u32::try_from(p).ok()) {
            let Some(process) = sys.process(sysinfo::Pid::from_u32(pid)) else {
                continue;
            };
            let member = Member {
                pid,
                start: process.start_time(),
            };
            if !self.members.contains(&member) {
                self.members.push(member);
            }
        }
    }

    #[cfg(not(windows))]
    fn add_job_members(&mut self, _sys: &System) {}

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
