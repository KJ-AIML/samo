CREATE TABLE IF NOT EXISTS audit_log (
    tenant_id TEXT NOT NULL,
    audit_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    surface TEXT NOT NULL,
    command_name TEXT NOT NULL,
    redacted_payload TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, audit_id)
);

CREATE INDEX IF NOT EXISTS idx_audit_log_tenant_created ON audit_log (tenant_id, created_at_ms DESC);

CREATE TABLE IF NOT EXISTS outbox_events (
    tenant_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_outbox_events_tenant_created ON outbox_events (tenant_id, created_at_ms DESC);
