//! The host against real helpers (the `samo-fake-helper` stand-in, on every OS): R8's limits, crashes, shutdown,
//! and turning off and on.

use samo_domain::extensions::BackoffConfig;
use samo_ext_protocol::{Widget, rows_in};
use samo_extensions::{
    ExtRoot, ExtSlot, ExtSnapshot, Host, HostConfig, MAX_ROWS, Registry, Status,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const HELLO: &str = "hello";
const WAIT_FOR_SHUTDOWN: &str = "wait-shutdown";
const SLOT: ExtSlot = ExtSlot(0);

struct Fixture {
    _dir: tempfile::TempDir,
    root: ExtRoot,
    dev: PathBuf,
}

/// A dev extension `test` whose helper is `script`.
fn fixture(script: &str) -> Result<Fixture, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dev = dir.path().join("dev/test");
    fs::create_dir_all(&dev).map_err(|e| e.to_string())?;
    fs::write(
        dev.join("extension.toml"),
        "id = \"test\"\nname = \"Test\"\nversion = \"0.1.0\"\nprotocol = 1\nmin_samo = \"0.1.0\"\n\
         helper = \"bin/helper\"\nsummary = \"A test\"\n\n[[changes]]\nwhat = \"Nothing\"\n\
         where = \"nowhere\"\nundo = \"nothing\"\n",
    )
    .map_err(|e| e.to_string())?;
    samo_testkit::fake_helper(
        Path::new(env!("CARGO_BIN_EXE_samo-fake-helper")),
        &dev.join("bin"),
        "helper",
        script,
    )
    .map_err(|e| e.to_string())?;
    let root = ExtRoot::at(dir.path().join("Extensions"));
    Ok(Fixture {
        _dir: dir,
        root,
        dev,
    })
}

fn config(dev: Option<&Path>) -> HostConfig {
    let mut c = HostConfig::new("0.1.0");
    c.dev_dir = dev.map(|d| d.to_string_lossy().into_owned());
    // Generous: a shell script's start can take a while on a busy machine or CI runner.
    c.hello_timeout = Duration::from_secs(2);
    c.grace = Duration::from_millis(300);
    c.one_shot_timeout = Duration::from_secs(5);
    c.backoff = BackoffConfig {
        first_ms: 20,
        max_ms: 100,
        max_crashes: 3,
        window_ms: 60_000,
    };
    c.wake_every = Duration::from_millis(10);
    c
}

fn start(f: &Fixture) -> Host {
    Host::start(f.root.clone(), config(Some(&f.dev)), Arc::new(|| {}))
}

fn wait_for(
    host: &Host,
    secs: u64,
    what: impl Fn(&ExtSnapshot) -> bool,
) -> Result<Arc<ExtSnapshot>, String> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let snap = host.snapshot();
        if what(&snap) {
            return Ok(snap);
        }
        if Instant::now() > deadline {
            return Err(format!(
                "timed out; last: {:?}",
                snap.get(SLOT).map(|e| (&e.status, e.badge, &e.note))
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn status(snap: &ExtSnapshot) -> Option<Status> {
    snap.get(SLOT).map(|e| e.status.clone())
}

#[test]
fn oversize_and_flood_are_dropped_not_buffered() -> Result<(), String> {
    let script = format!(
        r#"{HELLO}
say-x 100000
repeat 200 {{"type":"badge","count":{{i}},"summary":"n{{i}}"}}
repeat 30 {{"type":"popup","key":"k{{i}}","kind":"info","title":"t{{i}}"}}
{WAIT_FOR_SHUTDOWN}"#
    );
    let f = fixture(&script)?;
    let host = start(&f);
    // Only the latest badge is kept once the rate limit holds them back, and it does arrive.
    let snap = wait_for(&host, 10, |s| s.get(SLOT).is_some_and(|e| e.badge == 200))?;
    assert_eq!(status(&snap), Some(Status::Running));
    assert_eq!(
        snap.get(SLOT).map(|e| e.badge_summary.as_str()),
        Some("n200")
    );
    assert!(
        host.take_popups().len() <= 1,
        "pop-ups are 3 s apart at most"
    );
    let log = fs::read_to_string(f.root.data_dir("test").join("helper.log"))
        .map_err(|e| e.to_string())?;
    assert!(log.contains("dropped a 100000-byte line"), "{log}");
    host.shutdown_all();
    Ok(())
}

#[test]
fn view_rows_are_capped() -> Result<(), String> {
    let items: Vec<String> = (1..300)
        .map(|i| format!(r#"{{"lamp":"done","title":"r{i}"}}"#))
        .chain([r#"{"lamp":"done","title":"last"}"#.to_string()])
        .collect();
    let view = format!(
        r#"{{"type":"view","sections":[{{"widget":"rows","items":[{}]}}]}}"#,
        items.join(",")
    );
    let script = format!("{HELLO}\nsay {view}\n{WAIT_FOR_SHUTDOWN}");
    let f = fixture(&script)?;
    let host = start(&f);
    let snap = wait_for(&host, 10, |s| {
        s.get(SLOT).is_some_and(|e| !e.view.is_empty())
    })?;
    let view = snap.get(SLOT).map(|e| e.view.clone()).unwrap_or_default();
    assert!(rows_in(&view) <= MAX_ROWS + 1, "{}", rows_in(&view));
    assert!(matches!(view.last(), Some(Widget::Note { .. })));
    host.shutdown_all();
    Ok(())
}

#[test]
fn crash_backoff_reaches_stopped() -> Result<(), String> {
    let f = fixture("exit 3")?;
    let host = start(&f);
    let snap = wait_for(&host, 10, |s| matches!(status(s), Some(Status::Stopped(_))))?;
    let Some(Status::Stopped(why)) = status(&snap) else {
        return Err("not stopped".into());
    };
    assert!(why.contains("crashed 3 times"), "{why}");
    host.shutdown_all();
    Ok(())
}

#[test]
fn no_hello_in_time_is_stopped() -> Result<(), String> {
    let f = fixture("sleep 30")?;
    let host = start(&f);
    let snap = wait_for(&host, 10, |s| matches!(status(s), Some(Status::Stopped(_))))?;
    let Some(Status::Stopped(why)) = status(&snap) else {
        return Err("not stopped".into());
    };
    assert!(why.contains("didn't say hello"), "{why}");
    host.shutdown_all();
    Ok(())
}

#[test]
fn shutdown_kills_after_grace() -> Result<(), String> {
    // It never reads stdin, so it ignores `shutdown` and must be killed.
    let script = format!("write $DATA/pid $PID\n{HELLO}\nsleep 30");
    let f = fixture(&script)?;
    let host = start(&f);
    wait_for(&host, 10, |s| status(s) == Some(Status::Running))?;
    let pid = fs::read_to_string(f.root.data_dir("test").join("pid")).map_err(|e| e.to_string())?;
    let pid: u32 = pid.trim().parse().map_err(|e| format!("{e}: {pid}"))?;
    let started = Instant::now();
    host.shutdown_all();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert!(!alive(pid), "the helper is still running");
    Ok(())
}

/// Whether `pid` is a running process.
fn alive(pid: u32) -> bool {
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
fn helper_env_is_minimal() -> Result<(), String> {
    let script =
        format!("env-to $DATA/env.txt\ncwd-to $DATA/pwd.txt\n{HELLO}\n{WAIT_FOR_SHUTDOWN}");
    let f = fixture(&script)?;
    let host = start(&f);
    wait_for(&host, 10, |s| status(s) == Some(Status::Running))?;
    let data = f.root.data_dir("test");
    let env = fs::read_to_string(data.join("env.txt")).map_err(|e| e.to_string())?;
    let mut keys: Vec<&str> = env
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, _)| k))
        .collect();
    keys.sort_unstable();
    if cfg!(windows) {
        // Windows' own variables, only those Samo has itself, and nothing else of Samo's (ADR-006).
        let allowed = [
            "PATH",
            "SystemRoot",
            "SystemDrive",
            "windir",
            "ComSpec",
            "PATHEXT",
            "TEMP",
            "TMP",
            "APPDATA",
            "LOCALAPPDATA",
            "USERPROFILE",
            "SAMO_EXT_DATA_DIR",
        ];
        assert!(keys.iter().all(|k| allowed.contains(k)), "{env}");
        for needed in ["PATH", "SystemRoot", "USERPROFILE", "SAMO_EXT_DATA_DIR"] {
            assert!(keys.contains(&needed), "no {needed}: {env}");
        }
    } else {
        assert_eq!(
            keys,
            vec!["HOME", "LANG", "PATH", "SAMO_EXT_DATA_DIR"],
            "{env}"
        );
    }
    let pwd = fs::read_to_string(data.join("pwd.txt")).map_err(|e| e.to_string())?;
    let data_real = fs::canonicalize(&data).map_err(|e| e.to_string())?;
    let pwd_real = fs::canonicalize(pwd.trim()).map_err(|e| e.to_string())?;
    assert_eq!(pwd_real, data_real);
    host.shutdown_all();
    Ok(())
}

#[test]
fn off_runs_helper_off_then_stops() -> Result<(), String> {
    let script = format!(
        "@off\nwrite $DATA/off-ran x\n@on\nwrite $DATA/on-ran x\n@serve\n{HELLO}\n{WAIT_FOR_SHUTDOWN}"
    );
    let f = fixture(&script)?;
    let host = start(&f);
    wait_for(&host, 10, |s| status(s) == Some(Status::Running))?;
    let data = f.root.data_dir("test");

    host.set_on(SLOT, false);
    wait_for(&host, 10, |s| {
        status(s) == Some(Status::Off) && data.join("off-ran").exists()
    })?;
    // The registry and the history follow.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Registry::load(&f.root)
        .map_err(|e| e.to_string())?
        .get("test")
        .is_some_and(|i| i.on)
    {
        if Instant::now() > deadline {
            return Err("installed.toml still says on".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    // The history line can land a moment after the registry: wait for it too.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let history = fs::read_to_string(f.root.history_file()).unwrap_or_default();
        if history.contains("\"event\":\"off\"") {
            break;
        }
        if Instant::now() > deadline {
            return Err(format!("no off in the history: {history}"));
        }
        thread::sleep(Duration::from_millis(20));
    }

    host.set_on(SLOT, true);
    wait_for(&host, 10, |s| {
        status(s) == Some(Status::Running) && data.join("on-ran").exists()
    })?;
    host.shutdown_all();
    Ok(())
}

#[test]
fn dev_build_without_env_is_blocked() -> Result<(), String> {
    let script = format!("{HELLO}\n{WAIT_FOR_SHUTDOWN}");
    let f = fixture(&script)?;
    let host = start(&f);
    wait_for(&host, 10, |s| status(s) == Some(Status::Running))?;
    host.shutdown_all();

    // Started again without SAMO_EXT_DEV_DIR, the unsigned copy doesn't run.
    let again = Host::start(f.root.clone(), config(None), Arc::new(|| {}));
    let snap = wait_for(&again, 5, |s| matches!(status(s), Some(Status::Blocked(_))))?;
    assert!(snap.get(SLOT).is_some_and(|e| e.dev));
    again.shutdown_all();
    Ok(())
}
