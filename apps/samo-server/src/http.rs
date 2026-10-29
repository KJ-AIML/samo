use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use samo_blueprint::{DEFAULT_SAMPLE_INTERVAL_MS, ScanConfig, Surface};
use samo_engine::{DEFAULT_LOCAL_WEB_TOKEN, Engine};
use samo_macos::{
    SystemCollector, default_user_trash_dir, inspect_node_identity, inspect_process_identity,
    move_to_trash, scan_directory, terminate_verified_process,
};
use samo_primitives::{
    ErrorCode, FileNodeIdentity, PhosphorTheme, ProcessIdentity, SamoError, SamoResult, TenantId,
};
use samo_store::{StorePool, ensure_tenant, reconcile_interrupted_scans};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub engine: Engine,
    pub collector: Arc<Mutex<SystemCollector>>,
    pub trash_dir: PathBuf,
    pub default_scan_root: String,
    pub ui_dist_dir: Option<PathBuf>,
}

pub async fn initialize_server_state(
    pool: StorePool,
    trash_dir: PathBuf,
    default_scan_root: String,
    ui_dist_dir: Option<PathBuf>,
) -> SamoResult<AppState> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    {
        let mut sys_tx = pool.begin_system().await?;
        ensure_tenant(
            &mut sys_tx,
            &TenantId::local_default(),
            "Personal Mac",
            now_ms,
        )
        .await?;
        let _ = reconcile_interrupted_scans(&mut sys_tx, now_ms).await?;
        sys_tx.commit().await?;
    }

    let engine = Engine::new(pool);
    let collector = Arc::new(Mutex::new(SystemCollector::new(DEFAULT_SAMPLE_INTERVAL_MS)));

    Ok(AppState {
        engine,
        collector,
        trash_dir,
        default_scan_root,
        ui_dist_dir,
    })
}

pub fn build_router(state: AppState) -> Router {
    let api_router = Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/command", post(command_handler))
        .route("/api/telemetry/live", get(telemetry_live_handler))
        .route("/api/disk/scan", post(disk_scan_workflow_handler))
        .route("/api/disk/latest", get(disk_latest_handler))
        .route("/api/disk/history", get(disk_history_handler))
        .route("/api/disk/trash", post(disk_trash_workflow_handler))
        .route("/api/disk/reveal", post(disk_reveal_handler))
        .route(
            "/api/process/terminate",
            post(process_terminate_workflow_handler),
        )
        .route(
            "/api/settings",
            get(settings_get_handler).post(settings_set_handler),
        );

    let router = if let Some(dist) = &state.ui_dist_dir {
        let index_file = dist.join("index.html");
        api_router
            .fallback_service(ServeDir::new(dist).not_found_service(ServeFile::new(index_file)))
    } else {
        api_router
    };

    router.layer(CorsLayer::permissive()).with_state(state)
}

fn error_response(err: SamoError) -> (StatusCode, Json<Value>) {
    let status = match err.code {
        ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
        ErrorCode::PermissionDenied | ErrorCode::ProtectedPathDenied => StatusCode::FORBIDDEN,
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::InvalidInput
        | ErrorCode::InodeIdentityMismatch
        | ErrorCode::ProcessIdentityMismatch
        | ErrorCode::ScanAlreadyRunning => StatusCode::BAD_REQUEST,
        ErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(serde_json::json!({
            "error": {
                "code": err.code,
                "message": err.message,
            }
        })),
    )
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "default_scan_root": state.default_scan_root,
    }))
}

#[derive(Debug, Deserialize)]
pub struct GenericCommandRequest {
    pub command: String,
    pub payload: Value,
    pub token: Option<String>,
}

async fn command_handler(
    State(state): State<AppState>,
    Json(req): Json<GenericCommandRequest>,
) -> impl IntoResponse {
    let token = req.token.as_deref().unwrap_or(DEFAULT_LOCAL_WEB_TOKEN);
    match state
        .engine
        .execute(token, Surface::Web, &req.command, req.payload)
        .await
    {
        Ok(val) => (StatusCode::OK, Json(val)),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct TelemetryQuery {
    pub include_self: Option<bool>,
    pub search: Option<String>,
    pub sort_by: Option<String>,
    pub sample: Option<bool>,
}

async fn telemetry_live_handler(
    State(state): State<AppState>,
    Query(q): Query<TelemetryQuery>,
) -> impl IntoResponse {
    let include_self = q.include_self.unwrap_or(false);
    let search = q.search.as_deref().unwrap_or("");
    let sort_by = q.sort_by.as_deref().unwrap_or("cpu");
    let do_sample = q.sample.unwrap_or(true);

    let snap_res = state
        .collector
        .lock()
        .map_err(|_| SamoError::new(ErrorCode::InternalError, "collector lock poisoned"))
        .map(|mut guard| {
            if do_sample {
                let _ = guard.sample_tick();
            }
            guard.snapshot(include_self, search, sort_by)
        });

    match snap_res {
        Ok(snap) => match serde_json::to_value(snap) {
            Ok(val) => (StatusCode::OK, Json(val)),
            Err(e) => error_response(SamoError::new(ErrorCode::InternalError, e.to_string())),
        },
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize)]
pub struct TriggerScanRequest {
    pub root_path: Option<String>,
    pub max_threads: Option<usize>,
    pub top_files_limit: Option<usize>,
    pub top_folders_limit: Option<usize>,
}

/// 2-Phase Non-Transactional Disk Scan Workflow:
/// 1. `disk.scan.begin` (Transaction 1)
/// 2. `samo_macos::scan_directory` (Outside DB transaction)
/// 3. `disk.scan.finish` (Transaction 2)
async fn disk_scan_workflow_handler(
    State(state): State<AppState>,
    Json(req): Json<TriggerScanRequest>,
) -> impl IntoResponse {
    let root_path = req
        .root_path
        .unwrap_or_else(|| state.default_scan_root.clone());

    // Phase 1: disk.scan.begin
    let begin_val = match state
        .engine
        .execute(
            DEFAULT_LOCAL_WEB_TOKEN,
            Surface::Web,
            "disk.scan.begin",
            serde_json::json!({ "root_path": root_path }),
        )
        .await
    {
        Ok(v) => v,
        Err(err) => return error_response(err),
    };

    let scan_id = begin_val
        .get("scan_id")
        .and_then(Value::as_str)
        .unwrap_or("scn_unknown")
        .to_string();

    let scan_cfg = ScanConfig {
        max_threads: req.max_threads.unwrap_or(4),
        top_files_limit: req.top_files_limit.unwrap_or(100),
        top_folders_limit: req.top_folders_limit.unwrap_or(50),
    };

    // Phase 2: Filesystem walk outside DB transaction
    let root_path_clone = root_path.clone();
    let scan_id_clone = scan_id.clone();
    let walk_res = tokio::task::spawn_blocking(move || {
        scan_directory(scan_id_clone, Path::new(&root_path_clone), scan_cfg)
    })
    .await
    .map_err(|e| SamoError::new(ErrorCode::InternalError, e.to_string()));

    let snapshot = match walk_res {
        Ok(Ok(snap)) => snap,
        Ok(Err(err)) | Err(err) => return error_response(err),
    };

    // Phase 3: disk.scan.finish
    match state
        .engine
        .execute(
            DEFAULT_LOCAL_WEB_TOKEN,
            Surface::Web,
            "disk.scan.finish",
            serde_json::json!({ "snapshot": snapshot }),
        )
        .await
    {
        Ok(val) => (StatusCode::OK, Json(val)),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ScanLookupQuery {
    pub scan_id: Option<String>,
}

async fn disk_latest_handler(
    State(state): State<AppState>,
    Query(q): Query<ScanLookupQuery>,
) -> impl IntoResponse {
    match state
        .engine
        .execute(
            DEFAULT_LOCAL_WEB_TOKEN,
            Surface::Web,
            "disk.scan.get",
            serde_json::json!({ "scan_id": q.scan_id }),
        )
        .await
    {
        Ok(val) => (StatusCode::OK, Json(val)),
        Err(err) => error_response(err),
    }
}

async fn disk_history_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state
        .engine
        .execute(
            DEFAULT_LOCAL_WEB_TOKEN,
            Surface::Web,
            "disk.scan.list",
            serde_json::json!({}),
        )
        .await
    {
        Ok(val) => (StatusCode::OK, Json(val)),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Deserialize)]
pub struct TrashWorkflowRequest {
    pub scan_id: String,
    pub target_path: String,
    pub identity: FileNodeIdentity,
}

/// 2-Phase Non-Transactional Trash Cleanup Workflow (`R2`):
/// 1. `disk.cleanup.begin` (validates protected path denylist & inode match in Transaction 1)
/// 2. `samo_macos::move_to_trash` (collision-safe rename into `~/.Trash` outside DB transaction)
/// 3. `disk.cleanup.finish` (records trashed path & reclaimed bytes in Transaction 2)
async fn disk_trash_workflow_handler(
    State(state): State<AppState>,
    Json(req): Json<TrashWorkflowRequest>,
) -> impl IntoResponse {
    let live_identity = inspect_node_identity(Path::new(&req.target_path));

    let begin_val = match state
        .engine
        .execute(
            DEFAULT_LOCAL_WEB_TOKEN,
            Surface::Web,
            "disk.cleanup.begin",
            serde_json::json!({
                "scan_id": req.scan_id,
                "target_path": req.target_path,
                "scanned_identity": req.identity,
                "live_identity": live_identity,
            }),
        )
        .await
    {
        Ok(v) => v,
        Err(err) => return error_response(err),
    };

    let action_id = begin_val
        .get("action_id")
        .and_then(Value::as_str)
        .unwrap_or("cln_unknown")
        .to_string();

    let trash_outcome = move_to_trash(Path::new(&req.target_path), req.identity, &state.trash_dir);

    match trash_outcome {
        Ok(outcome) => match state
            .engine
            .execute(
                DEFAULT_LOCAL_WEB_TOKEN,
                Surface::Web,
                "disk.cleanup.finish",
                serde_json::json!({
                    "action_id": action_id,
                    "status": "completed",
                    "trashed_path": outcome.trashed_path,
                    "reclaimed_bytes": outcome.reclaimed_bytes,
                }),
            )
            .await
        {
            Ok(val) => (StatusCode::OK, Json(val)),
            Err(err) => error_response(err),
        },
        Err(err) => {
            let _ = state
                .engine
                .execute(
                    DEFAULT_LOCAL_WEB_TOKEN,
                    Surface::Web,
                    "disk.cleanup.finish",
                    serde_json::json!({
                        "action_id": action_id,
                        "status": "failed",
                        "trashed_path": Value::Null,
                        "reclaimed_bytes": 0,
                    }),
                )
                .await;
            error_response(err)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RevealRequest {
    pub path: String,
}

async fn disk_reveal_handler(Json(req): Json<RevealRequest>) -> impl IntoResponse {
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(&req.path)
        .status();
    match status {
        Ok(s) if s.success() => (
            StatusCode::OK,
            Json(serde_json::json!({ "revealed": req.path })),
        ),
        _ => error_response(SamoError::new(
            ErrorCode::NotFound,
            format!("could not reveal `{}` in Finder", req.path),
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct TerminateProcessRequest {
    pub identity: ProcessIdentity,
    pub signal: Option<String>,
    pub simulated_live_identity: Option<ProcessIdentity>,
}

/// 2-Phase Non-Transactional Process Termination Workflow (`R4`):
/// 1. `process.terminate.begin` (verifies `ProcessIdentity { pid, start_time_secs }` in Transaction 1)
/// 2. `samo_macos::terminate_verified_process` (re-verifies & sends signal outside DB transaction)
/// 3. `process.terminate.finish` (records result in Transaction 2)
async fn process_terminate_workflow_handler(
    State(state): State<AppState>,
    Json(req): Json<TerminateProcessRequest>,
) -> impl IntoResponse {
    let signal = req.signal.unwrap_or_else(|| "SIGTERM".to_string());
    let live_identity = req
        .simulated_live_identity
        .or_else(|| inspect_process_identity(req.identity.pid));

    let begin_val = match state
        .engine
        .execute(
            DEFAULT_LOCAL_WEB_TOKEN,
            Surface::Web,
            "process.terminate.begin",
            serde_json::json!({
                "requested_identity": req.identity,
                "live_identity": live_identity,
                "signal": signal,
            }),
        )
        .await
    {
        Ok(v) => v,
        Err(err) => return error_response(err),
    };

    let action_id = begin_val
        .get("action_id")
        .and_then(Value::as_str)
        .unwrap_or("prc_unknown")
        .to_string();

    let sig_res = terminate_verified_process(req.identity, &signal);
    let final_status = if sig_res.is_ok() {
        "completed"
    } else {
        "failed"
    };

    let _ = state
        .engine
        .execute(
            DEFAULT_LOCAL_WEB_TOKEN,
            Surface::Web,
            "process.terminate.finish",
            serde_json::json!({
                "action_id": action_id,
                "status": final_status,
            }),
        )
        .await;

    match sig_res {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "action_id": action_id,
                "pid": req.identity.pid,
                "signal": signal,
                "status": "completed",
            })),
        ),
        Err(err) => error_response(err),
    }
}

async fn settings_get_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state
        .engine
        .execute(
            DEFAULT_LOCAL_WEB_TOKEN,
            Surface::Web,
            "settings.get",
            serde_json::json!({}),
        )
        .await
    {
        Ok(val) => (StatusCode::OK, Json(val)),
        Err(err) => error_response(err),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetThemeRequest {
    pub theme: PhosphorTheme,
    pub include_self_process: Option<bool>,
}

async fn settings_set_handler(
    State(state): State<AppState>,
    Json(req): Json<SetThemeRequest>,
) -> impl IntoResponse {
    match state
        .engine
        .execute(
            DEFAULT_LOCAL_WEB_TOKEN,
            Surface::Web,
            "settings.theme.set",
            serde_json::json!({
                "theme": req.theme,
                "include_self_process": req.include_self_process,
            }),
        )
        .await
    {
        Ok(val) => (StatusCode::OK, Json(val)),
        Err(err) => error_response(err),
    }
}

pub fn resolve_default_trash_dir() -> PathBuf {
    default_user_trash_dir().unwrap_or_else(|_| PathBuf::from("/tmp/.samo-trash"))
}
