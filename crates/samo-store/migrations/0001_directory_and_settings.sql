CREATE TABLE IF NOT EXISTS tenants (
    tenant_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tenant_settings (
    tenant_id TEXT NOT NULL,
    theme TEXT NOT NULL DEFAULT 'dark',
    default_scan_root TEXT NOT NULL DEFAULT '/',
    sample_interval_ms INTEGER NOT NULL DEFAULT 1000,
    include_self_process INTEGER NOT NULL DEFAULT 0,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id)
);

CREATE INDEX IF NOT EXISTS idx_tenant_settings_tenant ON tenant_settings (tenant_id);
