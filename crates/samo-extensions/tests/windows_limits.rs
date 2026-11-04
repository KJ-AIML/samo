//! ADR-004 in the host: windows only when declared (R15), whole process trees, declared limits (R17), and macOS
//! versions. Helpers are the `samo-fake-helper` stand-in, on every OS; the window server is a fake, so a test can
//! say who "owns windows".

use samo_domain::extensions::BackoffConfig;
use samo_extensions::windows::{WindowCheck, WindowOwners};
use samo_extensions::{ExtRoot, ExtSlot, ExtSnapshot, Host, HostConfig, Status};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

type Fallible<T> = Result<T, String>;

const SLOT: ExtSlot = ExtSlot(0);
const HELLO: &str = "hello";
const WAIT_FOR_SHUTDOWN: &str = "wait-shutdown";

/// A window server that says what the test wants.
struct Fake(fn(&[u32]) -> Option<usize>);

impl WindowOwners for Fake {
    fn count(&self, pids: &[u32]) -> Option<usize> {
        (self.0)(pids)
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: ExtRoot,
    dev: PathBuf,
}

/// A dev extension `test` whose manifest has `extra` top-level lines (and tables after them), running `script`.
fn fixture(extra: &str, tables: &str, script: &str) -> Fallible<Fixture> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dev = dir.path().join("dev/test");
    fs::create_dir_all(&dev).map_err(|e| e.to_string())?;
    fs::write(
        dev.join("extension.toml"),
        format!(
            "id = \"test\"\nname = \"Test\"\nversion = \"0.1.0\"\nprotocol = 1\nmin_samo = \"0.1.0\"\n\
             helper = \"bin/helper\"\nsummary = \"A test\"\n{extra}\n{tables}\n\n[[changes]]\nwhat = \"Nothing\"\n\
             where = \"nowhere\"\nundo = \"nothing\"\n"
        ),
    )
    .map_err(|e| e.to_string())?;
    samo_testkit::fake_helper(
        Path::new(env!("CARGO_BIN_EXE_samo-fake-helper")),
        &dev.join("bin"),
        "helper",
        script,
    )
    .map_err(|e| e.to_string())?;
    Ok(Fixture {
        root: ExtRoot::at(dir.path().join("Extensions")),
        _dir: dir,
        dev,
    })
}

fn start(f: &Fixture, windows: fn(&[u32]) -> Option<usize>) -> Host {
    Host::start(f.root.clone(), config(f, windows), Arc::new(|| {}))
}

fn config(f: &Fixture, windows: fn(&[u32]) -> Option<usize>) -> HostConfig {
    let mut c = HostConfig::new("0.3.0");
    c.dev_dir = Some(f.dev.to_string_lossy().into_owned());
    c.hello_timeout = Duration::from_secs(2);
    c.grace = Duration::from_millis(300);
    c.backoff = BackoffConfig {
        first_ms: 20,
        max_ms: 100,
        max_crashes: 3,
        window_ms: 60_000,
    };
    c.wake_every = Duration::from_millis(10);
    c.resource_every = Duration::from_millis(50);
    c.window_every = Duration::from_millis(50);
    c.windows = WindowCheck(Arc::new(Fake(windows)));
    c.macos = Some("15.0".into());
    c
}

fn status(snap: &ExtSnapshot) -> Option<Status> {
    snap.get(SLOT).map(|e| e.status.clone())
}

fn wait_for(
    host: &Host,
    what: &str,
    test: impl Fn(&ExtSnapshot) -> bool,
) -> Fallible<Arc<ExtSnapshot>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snap = host.snapshot();
        if test(&snap) {
            return Ok(snap);
        }
        if Instant::now() > deadline {
            return Err(format!(
                "timed out waiting for {what}; status {:?}",
                status(&snap)
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn stopped_because(snap: &ExtSnapshot) -> Option<String> {
    match status(snap) {
        Some(Status::Stopped(why)) => Some(why),
        _ => None,
    }
}

/// Whether the process whose pid is written in `pid` is running.
fn alive(pid: &str) -> bool {
    let Ok(pid) = pid.trim().parse::<u32>() else {
        return false;
    };
    let mut sys = System::new();
    let pid = Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(pid).is_some()
}

#[test]
fn undeclared_windows_stop_the_helper() -> Fallible<()> {
    let f = fixture("", "", &format!("{HELLO}\n{WAIT_FOR_SHUTDOWN}"))?;
    let host = start(&f, |_| Some(1));
    let snap = wait_for(&host, "R15 to stop it", |s| stopped_because(s).is_some())?;
    assert_eq!(
        stopped_because(&snap).as_deref(),
        Some("Opened windows it didn't declare")
    );
    host.shutdown_all();
    Ok(())
}

#[test]
fn declared_windows_are_allowed() -> Fallible<()> {
    let f = fixture(
        "windows = \"Opens its own windows: a test window\"",
        "",
        &format!("{HELLO}\n{WAIT_FOR_SHUTDOWN}"),
    )?;
    let host = start(&f, |_| Some(1));
    wait_for(&host, "it to run", |s| status(s) == Some(Status::Running))?;
    // Many window checks later, it is still running.
    thread::sleep(Duration::from_millis(600));
    assert_eq!(status(&host.snapshot()), Some(Status::Running));
    let windows = host.snapshot().get(SLOT).and_then(|e| e.windows.clone());
    assert_eq!(
        windows.as_deref(),
        Some("Opens its own windows: a test window")
    );
    host.shutdown_all();
    Ok(())
}

/// The fake window server: windows exist only once the helper has started a copy of itself (its `spawn-sleeper`
/// child). On Windows a console program also has its console host in its family, which doesn't count.
fn has_child(pids: &[u32]) -> Option<usize> {
    let pids: Vec<Pid> = pids.iter().map(|p| Pid::from_u32(*p)).collect();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let sleeping = pids.iter().any(|pid| {
        sys.process(*pid)
            .is_some_and(|p| p.cmd().iter().any(|arg| arg == "__sleep"))
    });
    Some(usize::from(sleeping))
}

#[test]
fn descendants_windows_are_counted() -> Fallible<()> {
    let alone = fixture("", "", &format!("{HELLO}\n{WAIT_FOR_SHUTDOWN}"))?;
    let host = start(&alone, has_child);
    wait_for(&host, "it to run", |s| status(s) == Some(Status::Running))?;
    thread::sleep(Duration::from_millis(400));
    assert_eq!(
        status(&host.snapshot()),
        Some(Status::Running),
        "no child, no windows"
    );
    host.shutdown_all();

    let parent = fixture(
        "",
        "",
        &format!("spawn-sleeper 30\n{HELLO}\n{WAIT_FOR_SHUTDOWN}"),
    )?;
    let host = start(&parent, has_child);
    let snap = wait_for(&host, "R15 to stop it", |s| stopped_because(s).is_some())?;
    assert_eq!(
        stopped_because(&snap).as_deref(),
        Some("Opened windows it didn't declare")
    );
    host.shutdown_all();
    Ok(())
}

/// With the real window check, a helper that opens no window runs on. On Windows its family includes a hidden
/// console host, which mustn't count as a window.
#[cfg(any(target_os = "macos", windows))]
#[test]
fn a_helper_without_windows_runs_on_with_the_real_check() -> Fallible<()> {
    let f = fixture("", "", &format!("{HELLO}\n{WAIT_FOR_SHUTDOWN}"))?;
    let mut c = config(&f, |_| Some(0));
    c.windows = WindowCheck::default();
    let host = Host::start(f.root.clone(), c, Arc::new(|| {}));
    wait_for(&host, "it to run", |s| status(s) == Some(Status::Running))?;
    // Many checks, every 50 ms.
    thread::sleep(Duration::from_millis(1_000));
    assert_eq!(status(&host.snapshot()), Some(Status::Running));
    host.shutdown_all();
    Ok(())
}

/// R15 for real on Windows: the helper starts a program that shows a window, and Samo stops the helper and ends
/// what it started. It opens a window on the desktop, so it is run by hand (`-- --ignored`).
#[cfg(windows)]
#[test]
#[ignore = "opens a window on the desktop"]
fn an_undeclared_window_on_the_desktop_stops_the_tree() -> Fallible<()> {
    let root = std::env::var("SystemRoot").map_err(|e| e.to_string())?;
    let powershell = format!(r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe");
    let argv = serde_json::to_string(&[
        powershell.as_str(),
        "-NoProfile",
        "-Command",
        "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.MessageBox]::Show('Samo R15 test')",
    ])
    .map_err(|e| e.to_string())?;
    let f = fixture(
        "",
        "",
        &format!("spawn {argv}\n{HELLO}\n{WAIT_FOR_SHUTDOWN}"),
    )?;
    let mut c = config(&f, |_| Some(0));
    c.windows = WindowCheck::default();
    c.window_every = Duration::from_millis(500);
    let host = Host::start(f.root.clone(), c, Arc::new(|| {}));
    let snap = wait_for(&host, "R15 to stop it", |s| stopped_because(s).is_some())?;
    assert_eq!(
        stopped_because(&snap).as_deref(),
        Some("Opened windows it didn't declare")
    );
    let spawned =
        fs::read_to_string(f.root.data_dir("test").join("spawned")).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while alive(&spawned) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(&spawned), "the window's process outlived the helper");
    host.shutdown_all();
    Ok(())
}

#[test]
fn stop_kills_descendants() -> Fallible<()> {
    let script = format!("spawn-sleeper 300 $DATA/child\n{HELLO}\n{WAIT_FOR_SHUTDOWN}");
    let f = fixture("", "", &script)?;
    let host = start(&f, |_| Some(0));
    wait_for(&host, "it to run", |s| status(s) == Some(Status::Running))?;
    let child_file = f.root.data_dir("test").join("child");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !child_file.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let child = fs::read_to_string(&child_file).map_err(|e| e.to_string())?;
    // Let the supervisor see its tree at least once.
    thread::sleep(Duration::from_millis(200));
    assert!(alive(&child));

    // The helper exits cleanly on `shutdown`, leaving `sleep` behind; Samo ends it too.
    host.shutdown_all();
    let deadline = Instant::now() + Duration::from_secs(3);
    while alive(&child) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(&child), "the helper's child outlived it");
    Ok(())
}

#[test]
fn declared_limits_enforced() -> Fallible<()> {
    // A shell uses a few MB; declaring 1 MB proves the manifest's limit is the one enforced.
    let f = fixture(
        "",
        "[limits]\nmemory_mb = 1",
        &format!("{HELLO}\n{WAIT_FOR_SHUTDOWN}"),
    )?;
    let host = start(&f, |_| Some(0));
    let snap = wait_for(&host, "the memory limit", |s| stopped_because(s).is_some())?;
    let why = stopped_because(&snap).unwrap_or_default();
    assert!(why.contains("the limit is 1 MB"), "{why}");
    host.shutdown_all();
    Ok(())
}

/// About 20% of one core: busy for 20 ms, then asleep for 80 ms.
const LIGHT: &str = "light";

#[test]
fn cpu_is_measured_truly_beside_tree_checks() -> Fallible<()> {
    // R8/R17: the tree check (here every pass) once shared sysinfo's `System` with the CPU check, which then saw
    // several times the real use and stopped helpers well under their limit (devlog 0012).
    let f = fixture("", "[limits]\ncpu_pct = 50", &format!("{HELLO}\n{LIGHT}"))?;
    let mut c = config(&f, |_| Some(0));
    c.resource_every = Duration::from_secs(1);
    c.cpu_samples = 3;
    c.window_every = Duration::from_millis(20);
    let host = Host::start(f.root.clone(), c, Arc::new(|| {}));
    wait_for(&host, "it to run", |s| status(s) == Some(Status::Running))?;
    // Five checks: a full average of three, twice over.
    thread::sleep(Duration::from_millis(5_500));
    assert_eq!(status(&host.snapshot()), Some(Status::Running));
    host.shutdown_all();
    Ok(())
}

#[test]
fn over_its_limit_it_rests_and_starts_again_then_stays_stopped() -> Fallible<()> {
    // R8: going over a limit stops the helper, which starts again after a rest; the third time it stays stopped.
    let script = format!("write $DATA/pid $PID\n{HELLO}\nburn");
    let f = fixture("", "", &script)?;
    let mut c = config(&f, |_| Some(0));
    c.resource_every = Duration::from_millis(300);
    c.cpu_samples = 2;
    c.limit_backoff = BackoffConfig {
        first_ms: 600,
        max_ms: 600,
        max_crashes: 3,
        window_ms: 60_000,
    };
    let host = Host::start(f.root.clone(), c, Arc::new(|| {}));
    let pid_file = f.root.data_dir("test").join("pid");

    for _ in 0..2 {
        let snap = wait_for(&host, "the CPU limit", |s| stopped_because(s).is_some())?;
        let why = stopped_because(&snap).unwrap_or_default();
        assert!(
            why.starts_with("It used ")
                && why.contains("(the limit is 10%). Samo starts it again at "),
            "{why}"
        );
        let pid = fs::read_to_string(&pid_file).map_err(|e| e.to_string())?;
        assert!(!alive(&pid), "it rests stopped");
        fs::remove_file(&pid_file).map_err(|e| e.to_string())?;
        wait_for(&host, "it to start again", |s| {
            status(s) == Some(Status::Running)
        })?;
    }

    let snap = wait_for(&host, "it to stay stopped", |s| {
        stopped_because(s).is_some_and(|why| why.starts_with("It went over"))
    })?;
    let why = stopped_because(&snap).unwrap_or_default();
    assert!(
        why.starts_with("It went over its limits 3 times in 1 minutes. Last time it used "),
        "{why}"
    );
    thread::sleep(Duration::from_millis(1_000));
    assert_eq!(
        stopped_because(&host.snapshot()),
        Some(why),
        "no more restarts"
    );
    host.shutdown_all();
    Ok(())
}

#[test]
fn needs_newer_macos_is_blocked() -> Fallible<()> {
    let f = fixture(
        "min_macos = \"99.0\"",
        "",
        &format!("{HELLO}\n{WAIT_FOR_SHUTDOWN}"),
    )?;
    let host = start(&f, |_| Some(0));
    let snap = wait_for(&host, "it to be blocked", |s| {
        matches!(status(s), Some(Status::Blocked(_)))
    })?;
    // On Windows a manifest naming a macOS is a Mac build's (ADR-006).
    let why = if cfg!(windows) {
        "Made for the Mac: this is its Mac version"
    } else {
        "Needs macOS 99.0 or newer"
    };
    assert_eq!(status(&snap), Some(Status::Blocked(why.into())));
    host.shutdown_all();

    // And the store won't install it.
    let host = start(&f, |_| Some(0));
    let pkg = samo_domain::store::FileRef {
        file: "x.tar.gz".into(),
        size: None,
        sha256: "0".repeat(64),
        width: None,
        height: None,
    };
    let mut l = samo_testkit::store::listing("future", "0.1.0", pkg);
    l.min_macos = Some("99.0".into());
    let refused = host
        .install_from_store(&l, b"")
        .map(|_| ())
        .map_err(|e| e.to_string());
    // On Windows the listing's min_macos is about its Mac package; this one is refused for its empty package.
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| cfg!(windows) || e.contains("needs macOS 99.0")),
        "{refused:?}"
    );
    host.shutdown_all();
    Ok(())
}
