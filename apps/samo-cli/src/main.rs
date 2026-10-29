use samo_blueprint::ScanConfig;
use samo_macos::{SystemCollector, scan_directory};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1_024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("scan");

    match cmd {
        "scan" => {
            let target = args
                .get(1)
                .cloned()
                .or_else(|| env::var("HOME").ok())
                .unwrap_or_else(|| ".".to_string());
            let path = PathBuf::from(&target);
            println!("Scanning {} ...", path.display());
            let snap = scan_directory("scn_cli".to_string(), &path, ScanConfig::default())
                .map_err(|e| e.message)?;

            println!(
                "\nVolume: {} ({} used / {} free)",
                snap.volume_name,
                format_bytes(snap.volume_used_bytes),
                format_bytes(snap.volume_free_bytes)
            );
            println!(
                "Scanned: {} across {} files",
                format_bytes(snap.scanned_bytes),
                snap.scanned_files
            );

            println!("\n=== File Types ===");
            for cat in &snap.categories {
                println!(
                    "  {:<14} {:>12}  ({:>5.1}%)",
                    cat.label,
                    format_bytes(cat.bytes),
                    cat.share_pct
                );
            }

            if !snap.projects.is_empty() {
                println!("\n=== Dev Projects & Reclaimable Bloat ===");
                for proj in snap.projects.iter().take(15) {
                    println!(
                        "  {:<24} [{}] total {:>10} | reclaimable artifacts {:>10} ({})",
                        proj.name,
                        proj.kind,
                        format_bytes(proj.total_bytes),
                        format_bytes(proj.artifact_bytes),
                        proj.root_path
                    );
                }
            }

            println!("\n=== Largest Folders ===");
            for folder in snap.largest_folders.iter().take(10) {
                println!("  {:>12}  {}", format_bytes(folder.bytes), folder.path);
            }
            Ok(())
        }
        "top" => {
            let mut collector = SystemCollector::new(1000);
            std::thread::sleep(std::time::Duration::from_millis(250));
            let _ = collector.sample_tick();
            let snap = collector.snapshot(false, "", "cpu");
            println!(
                "Host: {} ({}) | Processes: {}",
                snap.system.hostname, snap.system.chip_brand, snap.total_processes
            );
            for p in snap.processes.iter().take(15) {
                println!(
                    "  PID {:<7} {:<24} CPU {:>6.1}%  MEM {:>10}",
                    p.identity.pid,
                    p.name,
                    p.cpu_pct,
                    format_bytes(p.memory_bytes)
                );
            }
            Ok(())
        }
        other => Err(format!(
            "unknown command `{other}` (use `samo-cli scan [path]` or `samo-cli top`)"
        )),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("samo-cli error: {e}");
            ExitCode::FAILURE
        }
    }
}
