//! A helper's CPU and memory (R8, R17), measured on a `System` of its own.
//!
//! sysinfo works a process's CPU use out from the time since its `System` last refreshed *any* process. The tree
//! check (R15) refreshes every process every 2 s, so a CPU check sharing its `System` divided 10 s of CPU time by
//! the under 2 s since, and saw 5 to 16 times the real use: Claude Sessions at 6% was stopped for "99%"
//! (devlog 0012). Nothing else refreshes a meter's `System`.

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

pub struct Meter {
    sys: System,
    pid: Pid,
}

impl Meter {
    /// Starts measuring `pid` now, so the first check covers the time since.
    pub fn start(pid: u32) -> Self {
        let mut meter = Self {
            sys: System::new(),
            pid: Pid::from_u32(pid),
        };
        // The first refresh only finds the process; the second notes its CPU time so far.
        meter.refresh();
        meter.refresh();
        meter
    }

    fn refresh(&mut self) {
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
    }

    /// Its resident memory (bytes), and its CPU (% of one core) since the last check. `None` once it is gone.
    pub fn check(&mut self) -> Option<(u64, f32)> {
        self.refresh();
        self.sys
            .process(self.pid)
            .map(|p| (p.memory(), p.cpu_usage()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    /// A program of the system's own, by its full path: `/usr/bin/yes`, or a file in Windows' System32.
    fn system(unix: &str, windows: &str) -> Command {
        if cfg!(windows) {
            let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
            Command::new(format!(r"{root}\System32\{windows}"))
        } else {
            Command::new(unix)
        }
    }

    /// Keeps one core busy.
    fn busy() -> Command {
        // cmd's endless `for /l` spins from its first millisecond; PowerShell can take seconds to start on a cold
        // CI runner, which the measurement then caught instead of the loop.
        let mut c = system("/usr/bin/yes", "cmd.exe");
        if cfg!(windows) {
            c.args(["/d", "/c", "for /l %i in () do rem"]);
        }
        c
    }

    /// Waits about 30 s, doing nothing.
    fn idle() -> Command {
        let mut c = system("/bin/sleep", "PING.EXE");
        if cfg!(windows) {
            c.args(["-n", "30", "127.0.0.1"]);
        } else {
            c.arg("30");
        }
        c
    }

    /// Measures `pid` for 1.5 s while another `System` walks every process each 50 ms, as the tree check does.
    fn measure_beside_tree_checks(pid: u32) -> Option<f32> {
        let mut meter = Meter::start(pid);
        let mut other = System::new();
        let until = Instant::now() + Duration::from_millis(1_500);
        while Instant::now() < until {
            let _ = tree::tree(&mut other, std::process::id());
            thread::sleep(Duration::from_millis(50));
        }
        meter.check().map(|(_, cpu)| cpu)
    }

    #[test]
    fn cpu_is_not_inflated_by_tree_checks() -> Result<(), String> {
        let mut busy = busy()
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut idle = idle()
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let busy_cpu = measure_beside_tree_checks(busy.id());
        let idle_cpu = measure_beside_tree_checks(idle.id());
        let _ = busy.kill();
        let _ = idle.kill();
        let _ = busy.wait();
        let _ = idle.wait();
        // One core busy: about 100%, never the 500% and more a shared `System` showed.
        let busy_cpu = busy_cpu.ok_or("lost the busy process")?;
        assert!((30.0..=150.0).contains(&busy_cpu), "busy: {busy_cpu}%");
        let idle_cpu = idle_cpu.ok_or("lost the idle process")?;
        assert!(idle_cpu < 5.0, "idle: {idle_cpu}%");
        Ok(())
    }

    #[test]
    fn gone_process_is_none() -> Result<(), String> {
        let mut child = idle().spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(Meter::start(pid).check(), None);
        Ok(())
    }
}
