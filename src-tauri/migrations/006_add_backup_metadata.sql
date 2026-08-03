-- Add the metadata required to validate a backup before it can be restored.

ALTER TABLE backup_runs ADD COLUMN kind TEXT NOT NULL DEFAULT 'automatic'
    CHECK (kind IN ('automatic', 'daily', 'session_close', 'pre_restore', 'manual_export'));

ALTER TABLE backup_runs ADD COLUMN managed INTEGER NOT NULL DEFAULT 1
    CHECK (managed IN (0, 1));

CREATE TABLE IF NOT EXISTS app_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO app_metadata (key, value) VALUES
    ('app_schema', 'unatienda'),
    ('backup_format', '1');

CREATE INDEX IF NOT EXISTS idx_backup_runs_managed_run_at
    ON backup_runs(managed, run_at);

INSERT OR IGNORE INTO schema_migrations (version) VALUES (6);
