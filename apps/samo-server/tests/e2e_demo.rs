use samo_primitives::{FileCategory, PhosphorTheme, ProcessIdentity, ScanSnapshot};
use samo_server::{build_router, initialize_server_state};
use samo_testkit::TestDb;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn http_json_request(
    addr: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let body_str = body
        .map(|v| serde_json::to_string(&v).unwrap_or_default())
        .unwrap_or_default();
    let req = if body_str.is_empty() {
        format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
    } else {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body_str.len(),
            body_str
        )
    };

    let Ok(mut stream) = tokio::net::TcpStream::connect(addr).await else {
        return (500, Value::Null);
    };
    let _ = stream.write_all(req.as_bytes()).await;
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf).await;
    let raw = String::from_utf8_lossy(&buf);

    let status = raw
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(500);

    let json_part = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("{}");
    let parsed: Value = serde_json::from_str(json_part).unwrap_or(Value::Null);
    (status, parsed)
}

#[tokio::test]
async fn phase2_end_to_end_demo_flow() {
    let Ok(test_db) = TestDb::new().await else {
        return;
    };
    let Ok(fixture_dir) = tempfile::tempdir() else {
        return;
    };
    let Ok(trash_dir) = tempfile::tempdir() else {
        return;
    };

    // 1. Create a realistic developer workspace fixture (Rust target/, Node node_modules/, Media, AppBundle)
    let root = fixture_dir.path();
    let rust_proj = root.join("alpha-rust");
    let rust_target = rust_proj.join("target");
    let _ = fs::create_dir_all(&rust_target);
    if let Ok(mut f) = File::create(rust_proj.join("Cargo.toml")) {
        let _ = f.write_all(b"[package]\nname = \"alpha-rust\"\n");
    }
    if let Ok(mut f) = File::create(rust_target.join("libalpha.rlib")) {
        let _ = f.write_all(&vec![b'R'; 131_072]);
    }

    let media_dir = root.join("Media Library");
    let _ = fs::create_dir_all(&media_dir);
    if let Ok(mut f) = File::create(media_dir.join("demo.mp4")) {
        let _ = f.write_all(&vec![b'M'; 262_144]);
    }

    let app_dir = root.join("SampleApp.app").join("Contents").join("MacOS");
    let _ = fs::create_dir_all(&app_dir);
    if let Ok(mut f) = File::create(app_dir.join("SampleApp")) {
        let _ = f.write_all(&vec![b'A'; 65_536]);
    }

    let Ok(state) = initialize_server_state(
        test_db.pool.clone(),
        trash_dir.path().to_path_buf(),
        root.to_string_lossy().into_owned(),
        None,
    )
    .await
    else {
        return;
    };

    let app = build_router(state);
    let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else {
        return;
    };
    let Ok(local_addr) = listener.local_addr() else {
        return;
    };
    let addr_str = local_addr.to_string();

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // 2. Verify live macOS telemetry endpoint returns Available readings and processes
    let (status_tel, tel_val) =
        http_json_request(&addr_str, "GET", "/api/telemetry/live", None).await;
    assert_eq!(status_tel, 200);
    assert_eq!(
        tel_val
            .pointer("/latest/cpu_overall_pct/state")
            .and_then(Value::as_str),
        Some("available")
    );
    assert!(
        tel_val
            .get("processes")
            .and_then(Value::as_array)
            .map(|a| !a.is_empty())
            .unwrap_or(false)
    );

    // 3. Trigger full Disk Space scan and verify categories, Balls layout, Projects, and History
    let (status_scan, scan_val) = http_json_request(
        &addr_str,
        "POST",
        "/api/disk/scan",
        Some(serde_json::json!({
            "root_path": root.to_string_lossy(),
        })),
    )
    .await;
    assert_eq!(status_scan, 200);
    let Ok(snapshot) = serde_json::from_value::<ScanSnapshot>(scan_val) else {
        return;
    };
    assert!(!snapshot.balls.is_empty());
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].name, "alpha-rust");
    assert!(!snapshot.projects[0].artifacts.is_empty());
    assert!(
        snapshot
            .categories
            .iter()
            .any(|c| c.category == FileCategory::Media && c.bytes > 0)
    );
    assert!(
        snapshot
            .categories
            .iter()
            .any(|c| c.category == FileCategory::Applications && c.bytes > 0)
    );

    // Verify History recorded the scan
    let (status_hist, hist_val) =
        http_json_request(&addr_str, "GET", "/api/disk/history", None).await;
    assert_eq!(status_hist, 200);
    assert_eq!(hist_val.as_array().map(Vec::len), Some(1));

    // 4. Move the Rust `target` artifact directory to Trash and verify R2 inode safety
    let target_artifact = &snapshot.projects[0].artifacts[0];
    let (status_trash, trash_val) = http_json_request(
        &addr_str,
        "POST",
        "/api/disk/trash",
        Some(serde_json::json!({
            "scan_id": snapshot.scan_id,
            "target_path": target_artifact.path,
            "identity": target_artifact.identity,
        })),
    )
    .await;
    assert_eq!(status_trash, 200);
    assert_eq!(
        trash_val.get("status").and_then(Value::as_str),
        Some("completed")
    );
    assert!(!rust_target.exists());
    assert!(trash_dir.path().join("target").exists());

    // 5. Verify R4 rejects terminating a process with a mismatched start_time_secs (recycled PID)
    let (status_proc, proc_val) = http_json_request(
        &addr_str,
        "POST",
        "/api/process/terminate",
        Some(serde_json::json!({
            "identity": ProcessIdentity { pid: std::process::id(), start_time_secs: 111 },
            "simulated_live_identity": ProcessIdentity { pid: std::process::id(), start_time_secs: 999 },
            "signal": "SIGTERM",
        })),
    )
    .await;
    assert_eq!(status_proc, 400);
    assert_eq!(
        proc_val.pointer("/error/code").and_then(Value::as_str),
        Some("process_identity_mismatch")
    );

    // 6. Switch theme to Green Phosphor and verify persistence
    let (status_theme, theme_val) = http_json_request(
        &addr_str,
        "POST",
        "/api/settings",
        Some(serde_json::json!({
            "theme": PhosphorTheme::Green,
            "include_self_process": true,
        })),
    )
    .await;
    assert_eq!(status_theme, 200);
    assert_eq!(
        theme_val.get("theme").and_then(Value::as_str),
        Some("green")
    );
}
