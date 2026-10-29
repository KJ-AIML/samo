CREATE TABLE IF NOT EXISTS disk_scans (
    tenant_id TEXT NOT NULL,
    scan_id TEXT NOT NULL,
    root_path TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    error_message TEXT,
    PRIMARY KEY (tenant_id, scan_id)
);

CREATE INDEX IF NOT EXISTS idx_disk_scans_tenant_started ON disk_scans (tenant_id, started_at_ms DESC);

CREATE TABLE IF NOT EXISTS disk_scan_snapshots (
    tenant_id TEXT NOT NULL,
    scan_id TEXT NOT NULL,
    root_path TEXT NOT NULL,
    volume_name TEXT NOT NULL,
    scanned_bytes INTEGER NOT NULL,
    scanned_files INTEGER NOT NULL,
    volume_used_bytes INTEGER NOT NULL,
    volume_total_bytes INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, scan_id)
);

CREATE INDEX IF NOT EXISTS idx_disk_snapshots_tenant_created ON disk_scan_snapshots (tenant_id, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS cleanup_actions (
    tenant_id TEXT NOT NULL,
    action_id TEXT NOT NULL,
    scan_id TEXT NOT NULL,
    target_path TEXT NOT NULL,
    device_id INTEGER NOT NULL,
    inode INTEGER NOT NULL,
    status TEXT NOT NULL,
    trashed_path TEXT,
    reclaimed_bytes INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    PRIMARY KEY (tenant_id, action_id)
);

CREATE INDEX IF NOT EXISTS idx_cleanup_actions_tenant_created ON cleanup_actions (tenant_id, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS process_actions (
    tenant_id TEXT NOT NULL,
    action_id TEXT NOT NULL,
    pid INTEGER NOT NULL,
    start_time_secs INTEGER NOT NULL,
    signal TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    PRIMARY KEY (tenant_id, action_id)
);

CREATE INDEX IF NOT EXISTS idx_process_actions_tenant_created ON process_actions (tenant_id, created_at_ms DESC);
