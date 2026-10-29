//! Everything slow happens off the UI thread.
//!
//! A sampler thread owns the `SystemCollector` (its refresh costs ~10 ms, which on the UI thread dropped a
//! frame every 1.2 s), publishes each snapshot into the shared state, and wakes the UI through a channel.
//! Scan threads wake it the same way when they finish. The UI never polls and never blocks on a sensor.

use crate::disk_view::DiskView;
use crate::{Notice, NoticeKind, ProcessSort};
use futures::channel::mpsc::UnboundedSender;
use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};
use samo_blueprint::DEFAULT_SAMPLE_INTERVAL_MS;
use samo_macos::{SystemCollector, TrashOutcome};
use samo_primitives::{
    Reading, SystemSummary, TelemetryPoint, TelemetrySnapshot, UnsupportedReason,
};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// How often sensors are read.
pub const SAMPLE_EVERY: Duration = Duration::from_millis(1200);
/// While a scan runs, the UI steps this often so Samo's probe can sweep (stepped, never 60 fps).
const SCAN_STEP: Duration = Duration::from_millis(150);

pub struct SharedState {
    pub scan_root: String,
    /// Bumped on every scan so a slow, superseded scan can't overwrite a newer one.
    pub scan_generation: u64,
    pub is_scanning: bool,
    pub scan_error: Option<String>,
    pub notice: Option<Notice>,
    pub last_trash: Option<TrashOutcome>,
    /// The last finished scan and what the pages derive from it.
    pub disk: Option<Arc<DiskView>>,
    /// Recent scans, newest first, so going into a folder and back out is instant.
    pub cache: Vec<Arc<DiskView>>,
    pub telemetry: Arc<TelemetrySnapshot>,
    /// The process view the telemetry was taken with, so a held table knows when it is stale.
    pub telemetry_view: (ProcessSort, bool),
    /// False until the first real sample lands.
    pub telemetry_ready: bool,
    /// The process using the most CPU across *all* processes (the table may be sorted another way).
    pub busiest: Option<(String, f32)>,
    /// How hot macOS says this Mac is running, and whether Low Power Mode is on.
    pub thermal: Option<Thermal>,
}

/// macOS's own thermal state: how far it is holding the Mac back to cool it down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heat {
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thermal {
    pub heat: Heat,
    pub low_power: bool,
}

/// One cheap call to `NSProcessInfo` (no sensors, no privileges), safe from any thread.
fn read_thermal() -> Thermal {
    objc2::rc::autoreleasepool(|_| {
        let info = NSProcessInfo::processInfo();
        let heat = match info.thermalState() {
            NSProcessInfoThermalState::Fair => Heat::Fair,
            NSProcessInfoThermalState::Serious => Heat::Serious,
            NSProcessInfoThermalState::Critical => Heat::Critical,
            _ => Heat::Nominal,
        };
        Thermal {
            heat,
            low_power: info.isLowPowerModeEnabled(),
        }
    })
}

struct Control {
    include_self: bool,
    sort: ProcessSort,
    /// Take a fresh process snapshot now (the view changed); doesn't force a sensor read.
    resnap: bool,
    /// Find the busiest process across all of them. It costs a second snapshot, so only while the quick
    /// panel (the one place that shows it) is open.
    want_busiest: bool,
}

/// The app's handle on its background work. Cheap to clone; every clone talks to the same threads.
#[derive(Clone)]
pub struct Hub {
    pub shared: Arc<Mutex<SharedState>>,
    control: Arc<(Mutex<Control>, Condvar)>,
    wake_ui: UnboundedSender<()>,
}

impl Hub {
    pub fn new(scan_root: String, wake_ui: UnboundedSender<()>) -> Self {
        Self {
            shared: Arc::new(Mutex::new(SharedState {
                scan_root,
                scan_generation: 0,
                is_scanning: false,
                scan_error: None,
                notice: None,
                last_trash: None,
                disk: None,
                cache: Vec::new(),
                telemetry: Arc::new(placeholder_telemetry()),
                telemetry_view: (ProcessSort::Memory, false),
                telemetry_ready: false,
                busiest: None,
                thermal: None,
            })),
            control: Arc::new((
                Mutex::new(Control {
                    include_self: false,
                    sort: ProcessSort::Memory,
                    resnap: false,
                    want_busiest: false,
                }),
                Condvar::new(),
            )),
            wake_ui,
        }
    }

    /// Ask the UI to draw again. Safe from any thread.
    pub fn wake_ui(&self) {
        let _ = self.wake_ui.unbounded_send(());
    }

    /// The process list should be sorted/filtered this way from now on.
    pub fn set_process_view(&self, include_self: bool, sort: ProcessSort) {
        let (lock, cvar) = &*self.control;
        if let Ok(mut c) = lock.lock() {
            c.include_self = include_self;
            c.sort = sort;
            c.resnap = true;
        }
        cvar.notify_one();
    }

    /// Turn the busiest-process lookup on (the quick panel opened) or off (it closed).
    pub fn want_busiest(&self, on: bool) {
        let (lock, cvar) = &*self.control;
        if let Ok(mut c) = lock.lock() {
            c.resnap |= on && !c.want_busiest;
            c.want_busiest = on;
        }
        cvar.notify_one();
    }

    /// Wake the sampler so it notices a scan has started (and begins stepping the UI).
    pub fn poke(&self) {
        self.control.1.notify_one();
    }

    pub fn is_scanning(&self) -> bool {
        self.shared.lock().map(|g| g.is_scanning).unwrap_or(false)
    }

    /// Starts the sampler thread. The collector is created there too, so launch doesn't wait on it.
    pub fn start_sampler(&self) {
        let hub = self.clone();
        let spawned = std::thread::Builder::new()
            .name("samo-sampler".into())
            .spawn(move || hub.sampler_loop());
        if spawned.is_err()
            && let Ok(mut g) = self.shared.lock()
        {
            g.notice = Notice::new(
                NoticeKind::Error,
                "Samo couldn’t start its sensors. Quit and open it again.",
            );
        }
    }

    fn sampler_loop(self) {
        let mut collector = SystemCollector::new(DEFAULT_SAMPLE_INTERVAL_MS);
        let mut next_sample = Instant::now() + SAMPLE_EVERY;
        let (mut include_self, mut sort, mut busiest) = (false, ProcessSort::Memory, false);
        self.publish(&collector, include_self, sort, busiest);
        if self.wake_ui.unbounded_send(()).is_err() {
            return;
        }

        loop {
            let scanning = self.is_scanning();
            let now = Instant::now();
            let until_sample = next_sample.saturating_duration_since(now);
            let wait = if scanning {
                until_sample.min(SCAN_STEP)
            } else {
                until_sample
            };

            let resnap = {
                let (lock, cvar) = &*self.control;
                let Ok(guard) = lock.lock() else { return };
                let Ok((mut c, _)) = cvar.wait_timeout_while(guard, wait, |c| !c.resnap) else {
                    return;
                };
                include_self = c.include_self;
                sort = c.sort;
                busiest = c.want_busiest;
                std::mem::take(&mut c.resnap)
            };

            let due = Instant::now() >= next_sample;
            if due {
                let _ = collector.sample_tick();
                next_sample = Instant::now() + SAMPLE_EVERY;
            }
            if due || resnap {
                self.publish(&collector, include_self, sort, busiest);
            }
            // UI gone: nothing left to sample for.
            if self.wake_ui.unbounded_send(()).is_err() {
                return;
            }
        }
    }

    fn publish(
        &self,
        collector: &SystemCollector,
        include_self: bool,
        sort: ProcessSort,
        want_busiest: bool,
    ) {
        let snap = Arc::new(collector.snapshot(include_self, "", sort.as_str()));
        let thermal = read_thermal();
        let busiest = if !want_busiest {
            None
        } else if sort == ProcessSort::Cpu {
            snap.processes.first().map(|p| (p.name.clone(), p.cpu_pct))
        } else {
            collector
                .snapshot(false, "", "cpu")
                .processes
                .first()
                .map(|p| (p.name.clone(), p.cpu_pct))
        };
        if let Ok(mut g) = self.shared.lock() {
            g.telemetry = snap;
            g.busiest = busiest;
            g.thermal = Some(thermal);
            g.telemetry_view = (sort, include_self);
            g.telemetry_ready = true;
        }
    }
}

/// What the UI shows before the first sample: every reading honestly "not sampled yet".
pub fn placeholder_telemetry() -> TelemetrySnapshot {
    fn not_yet<T>() -> Reading<T> {
        Reading::unsupported(UnsupportedReason::NotSampledYet)
    }
    TelemetrySnapshot {
        system: SystemSummary {
            hostname: String::new(),
            os_version: String::new(),
            chip_brand: "This Mac".to_string(),
            physical_cores: 0,
            logical_cores: 0,
            perf_cores: not_yet(),
            eff_cores: not_yet(),
            uptime_secs: 0,
            boot_reason: String::new(),
        },
        latest: TelemetryPoint {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            generation: 0,
            cpu_overall_pct: not_yet(),
            cpu_perf_cores_pct: not_yet(),
            cpu_eff_cores_pct: not_yet(),
            memory: not_yet(),
            disk_io: not_yet(),
            network: not_yet(),
        },
        history: Vec::new(),
        processes: Vec::new(),
        total_processes: 0,
        self_pid: std::process::id(),
        provider_healthy: true,
    }
}
