//! `samo-claude serve`: what Samo runs while Claude Sessions is on (ADR-002 C1–C3).
//!
//! It watches `data/sessions/` (one `stat` every 250 ms), checks Claude Code's processes every 5 s, reads the
//! day's transcripts within a budget, and picks up `limits.json`. It sends Samo the page and the badge only when
//! they change, and a pop-up when a session starts waiting or a long turn finishes. It exits when Samo says
//! `shutdown` or closes its stdin.

use crate::ancestry;
use crate::connection::{self, Places};
use crate::hook::now_ms;
use crate::limits::Limits;
use crate::paths;
use crate::popups::{Policy, Wanted};
use crate::sessions::{SessionFile, derive};
use crate::transcripts::Reader;
use crate::view::{self, Input, Switches};
use samo_ext_protocol::{
    BadgeMeter, HelperMsg, HostMsg, PROTOCOL_VERSION, decode_host_line, encode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, TryLockError};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const TICK: Duration = Duration::from_millis(250);
/// Transcripts read per tick: at most 8 MiB a second.
const READ_BUDGET: u64 = 2 * 1024 * 1024;
const LIVENESS_EVERY: Duration = Duration::from_secs(5);
const SETTINGS_EVERY: Duration = Duration::from_secs(5);
const SWEEP_EVERY: Duration = Duration::from_secs(5 * 60);
/// The page is sent at most this often when only durations changed.
const VIEW_EVERY: Duration = Duration::from_secs(1);

/// The pop-up switches, kept in `data/prefs.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Prefs {
    popup_needs_you: bool,
    popup_finished: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            popup_needs_you: true,
            popup_finished: true,
        }
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn modified_ms(path: &Path) -> i64 {
    modified(path)
        .and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// Local midnight today, in Unix ms.
fn local_midnight_ms() -> i64 {
    let now = chrono::Local::now();
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|m| m.and_local_timezone(chrono::Local).earliest())
        .map_or(0, |m| m.timestamp_millis())
}

fn time_of_day(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".into())
}

struct Serve {
    places: Places,
    sessions_dir: PathBuf,
    prefs: Prefs,
    visible: bool,
    known: HashMap<String, SessionFile>,
    loaded_once: bool,
    sessions_seen: Option<SystemTime>,
    alive: HashMap<String, bool>,
    last_liveness: Option<Instant>,
    reader: Reader,
    last_sweep: Option<Instant>,
    limits: Option<Limits>,
    limits_seen: Option<SystemTime>,
    active: connection::Active,
    last_settings: Option<Instant>,
    problem: Option<String>,
    policy: Policy,
    sent_view: String,
    sent_view_at: Option<Instant>,
    sent_badge: Option<(u32, String, Option<BadgeMeter>)>,
}

impl Serve {
    fn new(places: Places) -> Self {
        let prefs = fs::read(places.data.join("prefs.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let projects = places.home.join(".claude/projects");
        Self {
            sessions_dir: places.data.join("sessions"),
            reader: Reader::new(projects, local_midnight_ms()),
            places,
            prefs,
            visible: false,
            known: HashMap::new(),
            loaded_once: false,
            sessions_seen: None,
            alive: HashMap::new(),
            last_liveness: None,
            last_sweep: None,
            limits: None,
            limits_seen: None,
            active: connection::Active::default(),
            last_settings: None,
            problem: None,
            policy: Policy::default(),
            sent_view: String::new(),
            sent_view_at: None,
            sent_badge: None,
        }
    }

    fn save_prefs(&self) {
        if let Ok(text) = serde_json::to_vec(&self.prefs) {
            let _ = fs::write(self.places.data.join("prefs.json"), text);
        }
    }

    /// A switch was flipped on the page.
    fn setting(&mut self, key: &str, value: bool, out: &mut Vec<HelperMsg>) {
        let settings = self.places.settings().display().to_string();
        let result = match (key, value) {
            ("connect", true) => connection::connect(&self.places).map(|()| "connect"),
            ("connect", false) => connection::disconnect(&self.places).map(|()| "disconnect"),
            ("plan_limits", true) => {
                connection::plan_limits_on(&self.places).map(|()| "plan-limits-on")
            }
            ("plan_limits", false) => {
                connection::plan_limits_off(&self.places).map(|()| "plan-limits-off")
            }
            ("popup_needs_you", v) => {
                self.prefs.popup_needs_you = v;
                self.save_prefs();
                Ok("")
            }
            ("popup_finished", v) => {
                self.prefs.popup_finished = v;
                self.save_prefs();
                Ok("")
            }
            _ => Ok(""),
        };
        match result {
            Ok("") => {}
            Ok(event) => {
                self.problem = None;
                out.push(HelperMsg::Log {
                    event: event.to_string(),
                    detail: settings,
                });
            }
            Err(e) => self.problem = Some(format!("Couldn't change that: {e}.")),
        }
        // Read the settings again now, not in 5 s.
        self.last_settings = None;
        self.sent_view_at = None;
    }

    /// Reloads every session file; pop-ups for sessions that changed state.
    fn load_sessions(&mut self, now: i64, out: &mut Vec<HelperMsg>) -> bool {
        let mut next = HashMap::new();
        if let Ok(entries) = fs::read_dir(&self.sessions_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json")
                    && let Ok(bytes) = fs::read(&path)
                    && let Ok(file) = serde_json::from_slice::<SessionFile>(&bytes)
                {
                    next.insert(file.session_id.clone(), file);
                }
            }
        }
        let wanted = Wanted {
            needs_you: self.prefs.popup_needs_you && !self.visible,
            finished: self.prefs.popup_finished && !self.visible,
        };
        let mut changed = next.len() != self.known.len();
        for (id, file) in &next {
            let prev = self.known.get(id);
            changed |= prev.is_none_or(|p| p.state != file.state || p.reason != file.reason);
            if self.loaded_once
                && let Some(p) = self.policy.decide(prev, file, now, wanted)
            {
                out.push(HelperMsg::Popup {
                    key: p.key,
                    kind: p.kind,
                    title: p.title,
                    body: p.body,
                });
            }
            if !file.transcript_path.is_empty() {
                self.reader.follow(Path::new(&file.transcript_path));
            }
        }
        self.known = next;
        self.loaded_once = true;
        changed
    }

    /// Forgets sessions whose Claude Code process is gone, and their files.
    fn check_liveness(&mut self) -> bool {
        let ids: Vec<(String, samo_primitives::ProcessIdentity)> = self
            .known
            .iter()
            .filter_map(|(id, f)| f.claude.map(|c| (id.clone(), c)))
            .collect();
        let identities: Vec<_> = ids.iter().map(|(_, c)| *c).collect();
        let alive = ancestry::alive(&identities);
        let mut gone = false;
        self.alive.clear();
        for ((id, _), is_alive) in ids.into_iter().zip(alive) {
            if !is_alive {
                let _ = fs::remove_file(self.sessions_dir.join(format!("{id}.json")));
                let _ = fs::remove_file(self.sessions_dir.join(format!("{id}.lock")));
                self.known.remove(&id);
                gone = true;
            }
            self.alive.insert(id, is_alive);
        }
        gone
    }

    fn tick(&mut self, force: bool) -> Vec<HelperMsg> {
        let mut out = Vec::new();
        let now = now_ms();
        let mut state_changed = force;

        let seen = modified(&self.sessions_dir);
        if seen != self.sessions_seen || !self.loaded_once {
            self.sessions_seen = seen;
            state_changed |= self.load_sessions(now, &mut out);
        }
        if self
            .last_liveness
            .is_none_or(|t| t.elapsed() >= LIVENESS_EVERY)
        {
            self.last_liveness = Some(Instant::now());
            state_changed |= self.check_liveness();
        }
        if self
            .last_settings
            .is_none_or(|t| t.elapsed() >= SETTINGS_EVERY)
        {
            self.last_settings = Some(Instant::now());
            if let Ok(active) = connection::active(&self.places)
                && active != self.active
            {
                self.active = active;
                state_changed = true;
            }
        }
        self.reader.set_midnight(local_midnight_ms());
        if self.last_sweep.is_none_or(|t| t.elapsed() >= SWEEP_EVERY) {
            self.last_sweep = Some(Instant::now());
            self.reader.sweep();
        }
        self.reader.read_new(READ_BUDGET);
        let limits_path = self.places.data.join("limits.json");
        let limits_seen = modified(&limits_path);
        if limits_seen != self.limits_seen {
            self.limits_seen = limits_seen;
            self.limits = fs::read(&limits_path)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
        }

        let tally = self.reader.tally();
        let rows: Vec<_> = self
            .known
            .values()
            .filter_map(|f| {
                let activity = if f.transcript_path.is_empty() {
                    0
                } else {
                    modified_ms(Path::new(&f.transcript_path))
                };
                derive(f, now, self.alive.get(&f.session_id).copied(), activity)
                    .map(|r| (r, tally.per_session(&f.session_id)))
            })
            .collect();
        let view = view::build(&Input {
            rows: &rows,
            tokens: tally.reading(now),
            split: tally.total(),
            counting: self.reader.behind(),
            limits: self.limits,
            switches: Switches {
                connected: self.active.connected,
                plan_limits: self.active.plan_limits,
                other_statusline: self.active.other_statusline,
                popup_needs_you: self.prefs.popup_needs_you,
                popup_finished: self.prefs.popup_finished,
            },
            problem: self.problem.clone(),
            now_ms: now,
            clock: &time_of_day,
        });
        if let Ok(text) = encode(&view) {
            let due = self.sent_view_at.is_none_or(|t| t.elapsed() >= VIEW_EVERY);
            if text != self.sent_view && (state_changed || due) {
                self.sent_view = text;
                self.sent_view_at = Some(Instant::now());
                out.push(HelperMsg::View { sections: view });
            }
        }
        let (count, summary) = view::badge(&rows);
        let meter = view::badge_meter(self.active.plan_limits, self.limits.as_ref(), now);
        let badge = (count, summary, meter);
        if self.sent_badge.as_ref() != Some(&badge) {
            self.sent_badge = Some(badge.clone());
            out.push(HelperMsg::Badge {
                count: badge.0,
                summary: badge.1,
                meter: badge.2,
            });
        }
        out
    }
}

fn send(out: &mut impl Write, msg: &HelperMsg) -> bool {
    let Ok(line) = encode(msg) else { return true };
    writeln!(out, "{line}").and_then(|()| out.flush()).is_ok()
}

pub fn run() -> ExitCode {
    let (Some(data), Some(home)) = (paths::data_dir(), paths::home()) else {
        eprintln!("samo-claude: no home folder");
        return ExitCode::FAILURE;
    };
    if let Err(e) = fs::create_dir_all(&data) {
        eprintln!("samo-claude: can't create {}: {e}", data.display());
        return ExitCode::FAILURE;
    }
    let mut stdout = std::io::stdout().lock();
    if !send(
        &mut stdout,
        &HelperMsg::Hello {
            protocol: PROTOCOL_VERSION,
        },
    ) {
        return ExitCode::FAILURE;
    }

    // One copy at a time, even with two Samos running.
    let Ok(lock) = File::create(data.join("serve.lock")) else {
        return ExitCode::FAILURE;
    };
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            send(
                &mut stdout,
                &HelperMsg::Health {
                    ok: false,
                    message: "Claude Sessions is already running in another copy of Samo".into(),
                },
            );
            return ExitCode::SUCCESS;
        }
        Err(TryLockError::Error(e)) => {
            eprintln!("samo-claude: can't lock serve.lock: {e}");
            return ExitCode::FAILURE;
        }
    }

    let (tx, rx) = mpsc::channel::<Option<HostMsg>>();
    let _ = thread::Builder::new().name("stdin".into()).spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Ok(msg) = decode_host_line(&line)
                && tx.send(Some(msg)).is_err()
            {
                return;
            }
        }
        let _ = tx.send(None);
    });

    let mut serve = Serve::new(Places { home, data });
    let mut force = true;
    loop {
        match rx.recv_timeout(TICK) {
            Ok(Some(HostMsg::Shutdown)) | Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Some(HostMsg::Setting { key, value })) => {
                let mut out = Vec::new();
                serve.setting(&key, value, &mut out);
                for msg in &out {
                    if !send(&mut stdout, msg) {
                        return ExitCode::SUCCESS;
                    }
                }
                force = true;
            }
            Ok(Some(HostMsg::Visible { page })) => serve.visible = page,
            Ok(Some(HostMsg::Hello { .. } | HostMsg::Action { .. }))
            | Err(RecvTimeoutError::Timeout) => {}
        }
        for msg in serve.tick(force) {
            if !send(&mut stdout, &msg) {
                return ExitCode::SUCCESS;
            }
        }
        force = false;
    }
    drop(lock);
    ExitCode::SUCCESS
}
