use samo_blueprint::{DEFAULT_SAMPLE_INTERVAL_MS, MAX_RING_BUFFER_SAMPLES};
use samo_domain::{TelemetryRingBuffer, filter_and_sort_processes};
use samo_primitives::{
    DiskIoRates, EnergyImpact, MemoryBreakdown, MemoryPressure, NetworkRates, ProcessIdentity,
    ProcessRow, Reading, SystemSummary, TelemetryPoint, TelemetrySnapshot, UnsupportedReason,
};
use std::process::Command;
use sysinfo::{Networks, System};

pub struct SystemCollector {
    sys: System,
    networks: Networks,
    ring_buffer: TelemetryRingBuffer,
    generation: u64,
    last_sample_ms: Option<i64>,
    system_summary: SystemSummary,
    local_ip: String,
    self_pid: u32,
}

impl SystemCollector {
    pub fn new(sample_interval_ms: u64) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let networks = Networks::new_with_refreshed_list();
        let system_summary = detect_system_summary(&sys);
        let local_ip = detect_local_ip();
        let self_pid = std::process::id();

        let mut collector = Self {
            sys,
            networks,
            ring_buffer: TelemetryRingBuffer::new(
                MAX_RING_BUFFER_SAMPLES,
                sample_interval_ms.max(DEFAULT_SAMPLE_INTERVAL_MS),
            ),
            generation: 0,
            last_sample_ms: None,
            system_summary,
            local_ip,
            self_pid,
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

        let cpus = self.sys.cpus();
        let cpu_overall_pct = if cpus.is_empty() {
            Reading::unsupported(UnsupportedReason::SensorUnavailable)
        } else {
            let avg = cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / (cpus.len() as f32);
            Reading::available(avg, now_ms)
        };

        let (cpu_perf_cores_pct, cpu_eff_cores_pct) = match (
            self.system_summary.perf_cores.as_value().copied(),
            self.system_summary.eff_cores.as_value().copied(),
        ) {
            (Some(p_cnt), Some(e_cnt))
                if (p_cnt as usize + e_cnt as usize) <= cpus.len() && p_cnt > 0 =>
            {
                // On Apple Silicon macOS, efficiency or performance cores are grouped by topology
                let e_slice: Vec<f32> = cpus
                    .iter()
                    .take(e_cnt as usize)
                    .map(|c| c.cpu_usage())
                    .collect();
                let p_slice: Vec<f32> = cpus
                    .iter()
                    .skip(e_cnt as usize)
                    .take(p_cnt as usize)
                    .map(|c| c.cpu_usage())
                    .collect();
                (
                    Reading::available(p_slice, now_ms),
                    Reading::available(e_slice, now_ms),
                )
            }
            _ => (
                Reading::unsupported(UnsupportedReason::SensorUnavailable),
                Reading::unsupported(UnsupportedReason::SensorUnavailable),
            ),
        };

        let mem_breakdown = build_memory_breakdown(&self.sys, now_ms);
        let disk_rates = build_disk_io_rates(&self.sys, elapsed_secs);
        let net_rates = build_network_rates(&self.networks, &self.local_ip, elapsed_secs);

        self.system_summary.uptime_secs = System::uptime();

        let point = TelemetryPoint {
            timestamp_ms: now_ms,
            generation: self.generation,
            cpu_overall_pct,
            cpu_perf_cores_pct,
            cpu_eff_cores_pct,
            memory: Reading::available(mem_breakdown, now_ms),
            disk_io: Reading::available(disk_rates, now_ms),
            network: Reading::available(net_rates, now_ms),
        };

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

fn read_sysctl_u32(key: &str) -> Option<u32> {
    let out = Command::new("/usr/sbin/sysctl")
        .arg("-n")
        .arg(key)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u32>()
        .ok()
}

fn read_sysctl_string(key: &str) -> Option<String> {
    let out = Command::new("/usr/sbin/sysctl")
        .arg("-n")
        .arg(key)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if val.is_empty() { None } else { Some(val) }
}

fn detect_system_summary(sys: &System) -> SystemSummary {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let hostname = System::host_name().unwrap_or_else(|| "Mac.local".to_string());
    let os_version = format!(
        "{} {}",
        System::name().unwrap_or_else(|| "macOS".to_string()),
        System::os_version().unwrap_or_default()
    )
    .trim()
    .to_string();

    let chip_brand = read_sysctl_string("machdep.cpu.brand_string")
        .or_else(|| sys.cpus().first().map(|c| c.brand().to_string()))
        .unwrap_or_else(|| "Apple Silicon".to_string());

    let logical_cores = sys.cpus().len() as u32;
    let physical_cores = sys
        .physical_core_count()
        .map(|c| c as u32)
        .unwrap_or(logical_cores);

    // Apple Silicon exposes perflevel0 (Performance) and perflevel1 (Efficiency)
    let perf_cores = match read_sysctl_u32("hw.perflevel0.physicalcpu") {
        Some(n) if n > 0 => Reading::available(n, now_ms),
        _ => Reading::unsupported(UnsupportedReason::SensorUnavailable),
    };
    let eff_cores = match read_sysctl_u32("hw.perflevel1.physicalcpu") {
        Some(n) if n > 0 => Reading::available(n, now_ms),
        _ => Reading::unsupported(UnsupportedReason::SensorUnavailable),
    };

    let boot_reason = read_sysctl_string("kern.bootreason").unwrap_or_else(|| "user".to_string());

    SystemSummary {
        hostname,
        os_version,
        chip_brand,
        physical_cores,
        logical_cores,
        perf_cores,
        eff_cores,
        uptime_secs: System::uptime(),
        boot_reason,
    }
}

fn detect_local_ip() -> String {
    for iface in ["en0", "en1"] {
        if let Ok(out) = Command::new("/usr/sbin/ipconfig")
            .arg("getifaddr")
            .arg(iface)
            .output()
            && out.status.success()
        {
            let ip = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !ip.is_empty() {
                return ip;
            }
        }
    }
    "127.0.0.1".to_string()
}

fn parse_vm_stat_bytes() -> Option<(u64, u64, u64)> {
    let out = Command::new("/usr/bin/vm_stat").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut page_size: u64 = 16_384;
    let mut wired_pages: u64 = 0;
    let mut compressed_pages: u64 = 0;
    let mut cached_pages: u64 = 0;

    for line in text.lines() {
        if line.contains("page size of") {
            for token in line.split_whitespace() {
                if let Ok(ps) = token.parse::<u64>() {
                    page_size = ps;
                    break;
                }
            }
        } else if let Some((label, num_part)) = line.split_once(':') {
            let digits: String = num_part.chars().filter(|c| c.is_ascii_digit()).collect();
            let Ok(pages) = digits.parse::<u64>() else {
                continue;
            };
            let label_lower = label.trim().to_ascii_lowercase();
            if label_lower.contains("wired down") {
                wired_pages = pages;
            } else if label_lower.contains("occupied by compressor") {
                compressed_pages = pages;
            } else if label_lower.contains("file-backed pages") {
                cached_pages = pages;
            }
        }
    }

    Some((
        wired_pages.saturating_mul(page_size),
        compressed_pages.saturating_mul(page_size),
        cached_pages.saturating_mul(page_size),
    ))
}

fn build_memory_breakdown(sys: &System, now_ms: i64) -> MemoryBreakdown {
    let total_bytes = sys.total_memory();
    let used_bytes = sys.used_memory();
    let raw_avail = sys.available_memory();
    let available_bytes = if raw_avail > 0 {
        raw_avail
    } else {
        total_bytes.saturating_sub(used_bytes)
    };
    let swap_total_bytes = sys.total_swap();
    let swap_used_bytes = sys.used_swap();

    let (wired_bytes, compressed_bytes, cached_bytes) = match parse_vm_stat_bytes() {
        Some((w, c, f)) => (
            Reading::available(w, now_ms),
            Reading::available(c, now_ms),
            Reading::available(f, now_ms),
        ),
        None => (
            Reading::unsupported(UnsupportedReason::SensorUnavailable),
            Reading::unsupported(UnsupportedReason::SensorUnavailable),
            Reading::unsupported(UnsupportedReason::SensorUnavailable),
        ),
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
        wired_bytes,
        compressed_bytes,
        cached_bytes,
        swap_total_bytes,
        swap_used_bytes,
        pressure,
    }
}

fn build_disk_io_rates(sys: &System, elapsed_secs: f64) -> DiskIoRates {
    let mut read_delta: u64 = 0;
    let mut write_delta: u64 = 0;
    let mut total_read: u64 = 0;
    let mut total_write: u64 = 0;

    for proc_info in sys.processes().values() {
        let du = proc_info.disk_usage();
        read_delta = read_delta.saturating_add(du.read_bytes);
        write_delta = write_delta.saturating_add(du.written_bytes);
        total_read = total_read.saturating_add(du.total_read_bytes);
        total_write = total_write.saturating_add(du.total_written_bytes);
    }

    DiskIoRates {
        read_bytes_per_sec: (read_delta as f64 / elapsed_secs) as u64,
        write_bytes_per_sec: (write_delta as f64 / elapsed_secs) as u64,
        total_read_bytes: total_read,
        total_write_bytes: total_write,
    }
}

fn build_network_rates(networks: &Networks, local_ip: &str, elapsed_secs: f64) -> NetworkRates {
    let mut rx_delta: u64 = 0;
    let mut tx_delta: u64 = 0;
    let mut total_rx: u64 = 0;
    let mut total_tx: u64 = 0;
    let mut primary_interface = "en0".to_string();
    let mut max_traffic: u64 = 0;

    for (iface_name, data) in networks {
        if iface_name.starts_with("lo") {
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
        }
    }

    NetworkRates {
        rx_bytes_per_sec: (rx_delta as f64 / elapsed_secs) as u64,
        tx_bytes_per_sec: (tx_delta as f64 / elapsed_secs) as u64,
        total_rx_bytes: total_rx,
        total_tx_bytes: total_tx,
        primary_interface,
        local_ip: local_ip.to_string(),
    }
}

fn collect_process_rows(sys: &System, self_pid: u32) -> Vec<ProcessRow> {
    let mut rows = Vec::with_capacity(sys.processes().len());

    for (pid, proc_info) in sys.processes() {
        let raw_pid = pid.as_u32();
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

        let exe_path = proc_info
            .exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = proc_info.name().to_string_lossy().into_owned();
        if name.is_empty() {
            continue;
        }

        let user = proc_info
            .user_id()
            .map(|u| u.to_string())
            .unwrap_or_else(|| "system".to_string());

        rows.push(ProcessRow {
            identity: ProcessIdentity {
                pid: raw_pid,
                start_time_secs: proc_info.start_time(),
            },
            parent_pid: proc_info.parent().map(|p| p.as_u32()),
            name,
            executable_path: exe_path,
            user,
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
