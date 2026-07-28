-- Per-user reading state: key rows by (user_id, fingerprint) instead of
-- fingerprint alone, so each authorized user has independent progress/status.
--
-- Existing rows are assigned to the reserved local user ('local'): the desktop
-- GUI has no login and owns the local database. Remote users' devices hold
-- their own newest state and re-push it on the next sync (upsert keyed by
-- last_updated), so their server-side state self-heals after this migration.

CREATE TABLE reading_state_new (
    user_id           TEXT NOT NULL,
    fingerprint       TEXT NOT NULL REFERENCES contents (fingerprint) ON DELETE CASCADE,
    status            INTEGER NOT NULL DEFAULT 0,
    position          TEXT NOT NULL DEFAULT '{}',
    percentage        REAL NOT NULL DEFAULT 0.0,
    last_updated      TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z',
    status_updated_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z',
    PRIMARY KEY (user_id, fingerprint)
);

INSERT INTO reading_state_new
    (user_id, fingerprint, status, position, percentage, last_updated, status_updated_at)
SELECT 'local', fingerprint, status, position, percentage, last_updated, status_updated_at
FROM reading_state;

DROP TABLE reading_state;
ALTER TABLE reading_state_new RENAME TO reading_state;

CREATE INDEX idx_reading_state_fp ON reading_state (fingerprint);
