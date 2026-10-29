use samo_server::http::resolve_default_trash_dir;
use samo_server::{build_router, initialize_server_state};
use samo_store::StorePool;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn resolve_db_path() -> PathBuf {
    if let Ok(p) = env::var("SAMO_DB_PATH") {
        return PathBuf::from(p);
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".samo").join("samo.db");
    }
    PathBuf::from(".samo/samo.db")
}

fn resolve_default_scan_root() -> String {
    if let Ok(root) = env::var("SAMO_DEFAULT_SCAN_ROOT") {
        return root;
    }
    if let Ok(home) = env::var("HOME") {
        let dev = PathBuf::from(&home).join("Developer");
        if dev.is_dir() {
            return dev.to_string_lossy().into_owned();
        }
        return home;
    }
    "/".to_string()
}

fn resolve_ui_dist() -> Option<PathBuf> {
    if let Ok(custom) = env::var("SAMO_UI_DIST") {
        let p = PathBuf::from(custom);
        if p.is_dir() {
            return Some(p);
        }
    }
    for candidate in [
        "ui/dist",
        "repos/samo/ui/dist",
        "../ui/dist",
        "../../ui/dist",
    ] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

async fn run_server() -> Result<(), String> {
    let bind_addr = env::var("SAMO_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7420".to_string());
    let db_path = resolve_db_path();
    let default_scan_root = resolve_default_scan_root();
    let trash_dir = resolve_default_trash_dir();
    let ui_dist = resolve_ui_dist();

    let pool = StorePool::connect_file(&db_path)
        .await
        .map_err(|e| e.message)?;

    let state = initialize_server_state(pool, trash_dir, default_scan_root.clone(), ui_dist)
        .await
        .map_err(|e| e.message)?;

    // Background 1-second telemetry sampler so history is always warm
    let bg_collector = state.collector.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
        loop {
            interval.tick().await;
            if let Ok(mut guard) = bg_collector.lock() {
                let _ = guard.sample_tick();
            }
        }
    });

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| format!("failed to bind {bind_addr}: {e}"))?;

    println!("Samo instrument server listening on http://{bind_addr}");
    println!("Default scan root: {default_scan_root}");
    axum::serve(listener, app)
        .await
        .map_err(|e| format!("server error: {e}"))
}

#[tokio::main]
async fn main() -> ExitCode {
    match run_server().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("samo-server fatal error: {err}");
            ExitCode::FAILURE
        }
    }
}
