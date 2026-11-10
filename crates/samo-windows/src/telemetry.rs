//! Live telemetry on Windows, from `sysinfo`. What Windows can't tell Samo is `Unsupported`, never `0` (R3, ADR-006
//! R28): P and E cores, and wired, compressed and cached memory.
//!
//! The `sysinfo` parts read as `samo-macos/src/telemetry.rs` does; a collector the two share is left for later, so
//! the Mac's stays untouched (R25).

use samo_blueprint::{DEFAULT_SAMPLE_INTERVAL_MS, MAX_RING_BUFFER_SAMPLES};
use samo_domain::{TelemetryRingBuffer, filter_and_sort_processes, normalize_path};
use samo_primitives::{
    DiskIoRates, EnergyImpact, MemoryBreakdown, MemoryPressure, NetworkRates, ProcessIdentity,
    ProcessRow, Reading, SystemSummary, TelemetryPoint, TelemetrySnapshot, UnsupportedReason,
};
use std::net::IpAddr;
use sysinfo::{Disks, Networks, System};

fn not_on_windows<T>() -> Reading<T> {
    Reading::unsupported(UnsupportedReason::PlatformNotSupported)
}

/// Windows names its loopback adapter `Loopback Pseudo-Interface 1`.
fn is_loopback(interface: &str) -> bool {
    interface.to_ascii_lowercase().contains("loopback")
}

/// How long Samo waits for a sample before the charts show a gap. With every core busy, one reading here took up to
/// 2.1 s, so samples came about 3.3 s apart. Under the Mac's rule (twice the interval, 2 s), nearly every sample
/// became a gap and the charts went blank. A late sample is still a measurement, since `sysinfo` averages over the
/// time since the last one. Sleep, or a sampler that stopped, is a gap.
const GAP_AFTER_MS: u64 = 5_000;

fn ring_buffer(sample_interval_ms: u64) -> TelemetryRingBuffer {
    TelemetryRingBuffer::new(
        MAX_RING_BUFFER_SAMPLES,
        sample_interval_ms.max(DEFAULT_SAMPLE_INTERVAL_MS),
    )
    .with_gap_after(GAP_AFTER_MS)
}

/// A core's kind, as Windows reports it (`EfficiencyClass`, read through `gdt-cpus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Performance,
    /// Efficiency cores, the low-power ones too.
    Efficiency,
    Unknown,
}

/// Which of the CPUs Samo measures (`sysinfo`'s, in Windows' order) make up each performance and each efficiency
/// core. A performance core can run two threads; Samo shows a core's load, as the Mac does.
#[derive(Clone, Debug, Default, PartialEq)]
struct CoreKinds {
    performance: Vec<Vec<usize>>,
    efficiency: Vec<Vec<usize>>,
}

/// Groups logical processors `(Windows' number, core, kind)` into cores, in Windows' order. None if anything is
/// unknown: a kind Windows didn't name, a core whose threads disagree, or processors that aren't exactly the
/// `measured` CPUs numbered 0 to `measured - 1`. Never a guess (R28).
fn core_kinds_from(lps: &[(usize, u16, Kind)], measured: usize) -> Option<CoreKinds> {
    if lps.is_empty() || lps.len() != measured {
        return None;
    }
    let mut numbers: Vec<usize> = lps.iter().map(|lp| lp.0).collect();
    numbers.sort_unstable();
    if !numbers.iter().copied().eq(0..measured) {
        return None;
    }
    // Cores in the order their first thread comes, each with its kind and threads.
    let mut cores: Vec<(u16, Kind, Vec<usize>)> = Vec::new();
    for &(number, core, kind) in lps {
        if kind == Kind::Unknown {
            return None;
        }
        match cores.iter_mut().find(|c| c.0 == core) {
            Some(c) if c.1 != kind => return None,
            Some(c) => c.2.push(number),
            None => cores.push((core, kind, vec![number])),
        }
    }
    let mut kinds = CoreKinds::default();
    for (_, kind, threads) in cores {
        match kind {
            Kind::Performance => kinds.performance.push(threads),
            _ => kinds.efficiency.push(threads),
        }
    }
    Some(kinds)
}

/// Each core's load: the average of its threads'. None if a thread has no reading: unknown is never 0 (R3).
fn core_loads(cores: &[Vec<usize>], usage: &[f32]) -> Option<Vec<f32>> {
    cores
        .iter()
        .map(|threads| {
            let loads = threads
                .iter()
                .map(|&t| usage.get(t).copied())
                .collect::<Option<Vec<f32>>>()?;
            (!loads.is_empty()).then(|| loads.iter().sum::<f32>() / loads.len() as f32)
        })
        .collect()
}

/// What Windows says about this PC's cores, for the `measured` CPUs `sysinfo` reads.
fn detect_core_kinds(measured: usize) -> Option<CoreKinds> {
    let info = gdt_cpus::CpuInfo::detect().ok()?;
    let lps: Vec<(usize, u16, Kind)> = info
        .lps
        .iter()
        .map(|lp| {
            let kind = match lp.kind {
                gdt_cpus::CoreKind::Performance => Kind::Performance,
                gdt_cpus::CoreKind::Efficiency | gdt_cpus::CoreKind::LpEfficiency => {
                    Kind::Efficiency
                }
                _ => Kind::Unknown,
            };
            (usize::from(lp.os_id), lp.core, kind)
        })
        .collect();
    core_kinds_from(&lps, measured)
}

pub struct SystemCollector {
    sys: System,
    networks: Networks,
    disks: Disks,
    ring_buffer: TelemetryRingBuffer,
    generation: u64,
    last_sample_ms: Option<i64>,
    system_summary: SystemSummary,
    self_pid: u32,
    /// Which CPUs make up each performance and each efficiency core; none if Windows didn't say.
    core_kinds: Option<CoreKinds>,
}

impl SystemCollector {
    pub fn new(sample_interval_ms: u64) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let networks = Networks::new_with_refreshed_list();
        let disks = Disks::new_with_refreshed_list();
        let core_kinds = detect_core_kinds(sys.cpus().len());
        let mut system_summary = detect_system_summary(&sys);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let count = |cores: fn(&CoreKinds) -> &Vec<Vec<usize>>| match &core_kinds {
            Some(kinds) => Reading::available(cores(kinds).len() as u32, now_ms),
            None => Reading::unsupported(UnsupportedReason::SensorUnavailable),
        };
        system_summary.perf_cores = count(|k| &k.performance);
        system_summary.eff_cores = count(|k| &k.efficiency);
        let mut collector = Self {
            sys,
            networks,
            disks,
            ring_buffer: ring_buffer(sample_interval_ms),
            generation: 0,
            last_sample_ms: None,
            system_summary,
            self_pid: std::process::id(),
            core_kinds,
        };
        collector.sample_tick();
        collector
    }

    pub fn sample_tick(&mut self) -> TelemetryPoint {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let elapsed_secs = self
            .last_sample_ms
            .map(|prev| ((now_ms.saturating_sub(prev)) as f64 / 1000.0).max(0.2))
            .unwrap_or(1.0);
        self.last_sample_ms = Some(now_ms);
        self.generation = self.generation.saturating_add(1);

        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.sys
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        self.networks.refresh(true);
        self.disks.refresh(true);

        let cpus = self.sys.cpus();
        let cpu_overall_pct = if cpus.is_empty() {
            Reading::unsupported(UnsupportedReason::SensorUnavailable)
        } else {
            let avg = cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / (cpus.len() as f32);
            Reading::available(avg, now_ms)
        };
        let usage: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
        let loads = |cores: &[Vec<usize>]| match core_loads(cores, &usage) {
            Some(loads) => Reading::available(loads, now_ms),
            None => Reading::unsupported(UnsupportedReason::SensorUnavailable),
        };
        let (cpu_perf_cores_pct, cpu_eff_cores_pct) = match &self.core_kinds {
            Some(kinds) => (loads(&kinds.performance), loads(&kinds.efficiency)),
            None => (
                Reading::unsupported(UnsupportedReason::SensorUnavailable),
                Reading::unsupported(UnsupportedReason::SensorUnavailable),
            ),
        };

        let point = TelemetryPoint {
            timestamp_ms: now_ms,
            generation: self.generation,
            cpu_overall_pct,
            cpu_perf_cores_pct,
            cpu_eff_cores_pct,
            memory: Reading::available(build_memory_breakdown(&self.sys), now_ms),
            disk_io: Reading::available(build_disk_io_rates(&self.disks, elapsed_secs), now_ms),
            network: Reading::available(build_network_rates(&self.networks, elapsed_secs), now_ms),
        };
        self.system_summary.uptime_secs = System::uptime();
        self.ring_buffer.append_sample(point.clone());
        point
    }

    pub fn snapshot(&self, include_self: bool, search: &str, sort_by: &str) -> TelemetrySnapshot {
        let all_procs = collect_process_rows(&self.sys, self.self_pid);
        let total_processes = all_procs.len();
        let mut filtered = filter_and_sort_processes(&all_procs, include_self, search, sort_by);
        filtered.truncate(150);

        let latest = self.ring_buffer.latest().unwrap_or_else(|| TelemetryPoint {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            generation: self.generation,
            cpu_overall_pct: Reading::unsupported(UnsupportedReason::NotSampledYet),
            cpu_perf_cores_pct: Reading::unsupported(UnsupportedReason::NotSampledYet),
            cpu_eff_cores_pct: Reading::unsupported(UnsupportedReason::NotSampledYet),
            memory: Reading::unsupported(UnsupportedReason::NotSampledYet),
            disk_io: Reading::unsupported(UnsupportedReason::NotSampledYet),
            network: Reading::unsupported(UnsupportedReason::NotSampledYet),
        });

        TelemetrySnapshot {
            system: self.system_summary.clone(),
            latest,
            history: self.ring_buffer.snapshot_vec(),
            processes: filtered,
            total_processes,
            self_pid: self.self_pid,
            provider_healthy: true,
        }
    }
}

fn detect_system_summary(sys: &System) -> SystemSummary {
    let os_version = format!(
        "{} {}",
        System::name().unwrap_or_else(|| "Windows".to_string()),
        System::os_version().unwrap_or_default()
    )
    .trim()
    .to_string();
    let logical_cores = sys.cpus().len() as u32;
    SystemSummary {
        hostname: System::host_name().unwrap_or_else(|| "This PC".to_string()),
        os_version,
        chip_brand: sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| "Unknown processor".to_string()),
        physical_cores: sys
            .physical_core_count()
            .map(|c| c as u32)
            .unwrap_or(logical_cores),
        logical_cores,
        perf_cores: not_on_windows(),
        eff_cores: not_on_windows(),
        uptime_secs: System::uptime(),
        boot_reason: "unknown".to_string(),
    }
}

fn build_memory_breakdown(sys: &System) -> MemoryBreakdown {
    let total_bytes = sys.total_memory();
    let used_bytes = sys.used_memory();
    let raw_avail = sys.available_memory();
    let available_bytes = if raw_avail > 0 {
        raw_avail
    } else {
        total_bytes.saturating_sub(used_bytes)
    };
    let usage_ratio = if total_bytes > 0 {
        used_bytes as f64 / total_bytes as f64
    } else {
        0.0
    };
    let pressure = if usage_ratio > 0.90 {
        MemoryPressure::Critical
    } else if usage_ratio > 0.75 {
        MemoryPressure::Elevated
    } else {
        MemoryPressure::Normal
    };
    MemoryBreakdown {
        total_bytes,
        used_bytes,
        available_bytes,
        wired_bytes: not_on_windows(),
        compressed_bytes: not_on_windows(),
        cached_bytes: not_on_windows(),
        swap_total_bytes: sys.total_swap(),
        swap_used_bytes: sys.used_swap(),
        pressure,
    }
}

/// The drives' own read and write counters. A process's I/O on Windows counts the network and devices too, so the
/// sum over processes isn't disk activity.
fn build_disk_io_rates(disks: &Disks, elapsed_secs: f64) -> DiskIoRates {
    let (mut read_delta, mut write_delta, mut total_read, mut total_write) =
        (0u64, 0u64, 0u64, 0u64);
    for disk in disks.list() {
        let usage = disk.usage();
        read_delta = read_delta.saturating_add(usage.read_bytes);
        write_delta = write_delta.saturating_add(usage.written_bytes);
        total_read = total_read.saturating_add(usage.total_read_bytes);
        total_write = total_write.saturating_add(usage.total_written_bytes);
    }
    DiskIoRates {
        read_bytes_per_sec: (read_delta as f64 / elapsed_secs) as u64,
        write_bytes_per_sec: (write_delta as f64 / elapsed_secs) as u64,
        total_read_bytes: total_read,
        total_write_bytes: total_write,
    }
}

fn build_network_rates(networks: &Networks, elapsed_secs: f64) -> NetworkRates {
    let (mut rx_delta, mut tx_delta, mut total_rx, mut total_tx) = (0u64, 0u64, 0u64, 0u64);
    let mut primary_interface = String::new();
    let mut local_ip = String::new();
    let mut max_traffic: u64 = 0;

    for (iface_name, data) in networks {
        if is_loopback(iface_name) {
            continue;
        }
        rx_delta = rx_delta.saturating_add(data.received());
        tx_delta = tx_delta.saturating_add(data.transmitted());
        total_rx = total_rx.saturating_add(data.total_received());
        total_tx = total_tx.saturating_add(data.total_transmitted());
        let iface_total = data
            .total_received()
            .saturating_add(data.total_transmitted());
        if iface_total > max_traffic {
            max_traffic = iface_total;
            primary_interface = iface_name.clone();
            // The busiest adapter's own IPv4 address: read from Windows, no packet sent.
            if let Some(ip) = data.ip_networks().iter().find_map(|n| match n.addr {
                IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_link_local() => Some(v4),
                _ => None,
            }) {
                local_ip = ip.to_string();
            }
        }
    }

    NetworkRates {
        rx_bytes_per_sec: (rx_delta as f64 / elapsed_secs) as u64,
        tx_bytes_per_sec: (tx_delta as f64 / elapsed_secs) as u64,
        total_rx_bytes: total_rx,
        total_tx_bytes: total_tx,
        primary_interface,
        local_ip: if local_ip.is_empty() {
            "127.0.0.1".to_string()
        } else {
            local_ip
        },
    }
}

fn collect_process_rows(sys: &System, self_pid: u32) -> Vec<ProcessRow> {
    let mut rows = Vec::with_capacity(sys.processes().len());
    for (pid, proc_info) in sys.processes() {
        let raw_pid = pid.as_u32();
        let name = proc_info.name().to_string_lossy().into_owned();
        if name.is_empty() {
            continue;
        }
        let cpu_pct = proc_info.cpu_usage();
        let du = proc_info.disk_usage();
        let energy_impact = if cpu_pct > 80.0 {
            EnergyImpact::VeryHigh
        } else if cpu_pct > 30.0 {
            EnergyImpact::High
        } else if cpu_pct > 5.0 {
            EnergyImpact::Moderate
        } else {
            EnergyImpact::Low
        };
        rows.push(ProcessRow {
            identity: ProcessIdentity {
                pid: raw_pid,
                start_time_secs: proc_info.start_time(),
            },
            parent_pid: proc_info.parent().map(|p| p.as_u32()),
            name,
            executable_path: proc_info
                .exe()
                .map(|p| normalize_path(&p.to_string_lossy()))
                .unwrap_or_default(),
            user: proc_info
                .user_id()
                .map(|u| u.to_string())
                .unwrap_or_else(|| "system".to_string()),
            cpu_pct,
            memory_bytes: proc_info.memory(),
            disk_read_bytes_per_sec: du.read_bytes,
            disk_write_bytes_per_sec: du.written_bytes,
            energy_impact,
            is_self: raw_pid == self_pid,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn says_unsupported<T>(reading: &Reading<T>) -> bool {
        matches!(
            reading,
            Reading::Unsupported {
                reason: UnsupportedReason::PlatformNotSupported
            }
        )
    }

    /// With every core busy, a reading here took up to 2.1 s, so samples came 3.3 s apart: under the old 2 s rule
    /// nearly every one was called a gap, and the charts went blank. A late sample is still measured.
    #[test]
    fn a_late_sample_on_a_busy_pc_is_not_a_gap() {
        let point = |ts: i64| TelemetryPoint {
            timestamp_ms: ts,
            generation: 1,
            cpu_overall_pct: Reading::available(97.0, ts),
            cpu_perf_cores_pct: not_on_windows(),
            cpu_eff_cores_pct: not_on_windows(),
            memory: not_on_windows(),
            disk_io: not_on_windows(),
            network: not_on_windows(),
        };
        let mut buffer = ring_buffer(DEFAULT_SAMPLE_INTERVAL_MS);
        for ts in [0, 3_300, 6_600, 9_900] {
            buffer.append_sample(point(ts));
        }
        assert_eq!(buffer.len(), 4, "no gaps between late samples");
        // Asleep, or Samo stopped sampling: that is a gap.
        buffer.append_sample(point(9_900 + 6_000));
        let stopped = buffer
            .snapshot_vec()
            .iter()
            .filter(|p| matches!(p.cpu_overall_pct, Reading::Stopped { .. }))
            .count();
        assert_eq!(stopped, 1);
    }

    /// KJ's i7-1255U, as Windows reports it: two performance cores of two threads each, then eight efficiency cores.
    fn i7_1255u() -> Vec<(usize, u16, Kind)> {
        let mut lps = vec![
            (0, 0, Kind::Performance),
            (1, 0, Kind::Performance),
            (2, 1, Kind::Performance),
            (3, 1, Kind::Performance),
        ];
        lps.extend((4..12).map(|lp| (lp, lp as u16 - 2, Kind::Efficiency)));
        lps
    }

    #[test]
    fn a_hybrid_cpus_threads_are_grouped_into_its_cores() {
        let cores = core_kinds_from(&i7_1255u(), 12);
        assert_eq!(
            cores,
            Some(CoreKinds {
                performance: vec![vec![0, 1], vec![2, 3]],
                efficiency: (4..12).map(|lp| vec![lp]).collect(),
            })
        );
    }

    #[test]
    fn a_cores_load_is_its_threads_average() -> Result<(), String> {
        let cores = core_kinds_from(&i7_1255u(), 12).ok_or("no cores")?;
        let usage = [
            100.0, 50.0, 0.0, 20.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0,
        ];
        assert_eq!(
            core_loads(&cores.performance, &usage),
            Some(vec![75.0, 10.0])
        );
        assert_eq!(
            core_loads(&cores.efficiency, &usage),
            Some(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])
        );
        // A thread with no reading: the loads are unknown, never 0.
        assert_eq!(core_loads(&cores.efficiency, &usage[..6]), None);
        Ok(())
    }

    /// Never a guess (R28): a kind Windows didn't name, or processors that don't line up with the CPUs Samo measures,
    /// and it is said unknown.
    #[test]
    fn what_windows_didnt_say_isnt_guessed() {
        let mut unknown = i7_1255u();
        unknown[5].2 = Kind::Unknown;
        assert_eq!(core_kinds_from(&unknown, 12), None);
        // Fewer processors than CPUs measured, or numbered otherwise.
        assert_eq!(core_kinds_from(&i7_1255u(), 16), None);
        let mut renumbered = i7_1255u();
        renumbered[0].0 = 40;
        assert_eq!(core_kinds_from(&renumbered, 12), None);
        // A core whose threads disagree.
        let mut mixed = i7_1255u();
        mixed[1].2 = Kind::Efficiency;
        assert_eq!(core_kinds_from(&mixed, 12), None);
        assert_eq!(core_kinds_from(&[], 0), None);
    }

    /// A CPU of one kind (most desktops and servers) has only performance cores, each with its load.
    #[test]
    fn a_cpu_of_one_kind_has_performance_cores_only() -> Result<(), String> {
        let lps: Vec<(usize, u16, Kind)> = (0..8)
            .map(|lp| (lp, lp as u16 / 2, Kind::Performance))
            .collect();
        let cores = core_kinds_from(&lps, 8).ok_or("no cores")?;
        assert_eq!(cores.performance.len(), 4);
        assert!(cores.efficiency.is_empty());
        Ok(())
    }

    #[test]
    fn readings_are_real_or_said_unsupported() -> Result<(), String> {
        let mut collector = SystemCollector::new(1000);
        let point = collector.sample_tick();
        assert!(point.cpu_overall_pct.as_value().is_some());
        let memory = point.memory.as_value().ok_or("no memory reading")?;
        assert!(memory.total_bytes > 0);
        assert!(says_unsupported(&memory.wired_bytes));
        assert!(says_unsupported(&memory.compressed_bytes));
        assert!(says_unsupported(&memory.cached_bytes));

        let snapshot = collector.snapshot(true, "", "cpu");
        // Cores of each kind: as many loads as Windows said cores, or said unknown together.
        let per_core = [
            (&snapshot.system.perf_cores, &point.cpu_perf_cores_pct),
            (&snapshot.system.eff_cores, &point.cpu_eff_cores_pct),
        ];
        for (count, loads) in per_core {
            match (count.as_value(), loads.as_value()) {
                (Some(&n), Some(loads)) => {
                    assert_eq!(loads.len(), n as usize);
                    assert!(loads.iter().all(|l| (0.0..=100.0).contains(l)));
                }
                (None, None) => {}
                other => return Err(format!("count and loads disagree: {other:?}")),
            }
        }
        assert!(snapshot.system.logical_cores > 0);
        assert!(snapshot.processes.iter().any(|p| p.is_self));
        // Paths leave samo-windows in the one shape (ADR-006 R26).
        for row in &snapshot.processes {
            assert!(
                !row.executable_path.contains('\\'),
                "{}",
                row.executable_path
            );
        }
        Ok(())
    }

    /// Disk activity is the drives' own counters, not every process's I/O (which on Windows counts the network too).
    #[test]
    fn disk_activity_comes_from_the_drives() {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let rates = build_disk_io_rates(&disks, 1.0);
        assert!(
            rates.total_read_bytes > 0,
            "the system drive has been read since boot"
        );
    }
}
