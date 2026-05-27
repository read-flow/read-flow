PRAGMA foreign_keys = OFF;

CREATE TABLE reading_state (
    fingerprint       TEXT NOT NULL PRIMARY KEY
                          REFERENCES contents (fingerprint) ON DELETE CASCADE,
    status            INTEGER NOT NULL DEFAULT 0,
    position          TEXT NOT NULL DEFAULT '{}',
    percentage        REAL NOT NULL DEFAULT 0.0,
    last_updated      TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z',
    status_updated_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z'
);

-- Phase 1: Migrate non-Unread status from contents.
INSERT OR IGNORE INTO reading_state (fingerprint, status, status_updated_at, last_updated)
SELECT fingerprint,
       status,
       strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
       strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
FROM contents
WHERE status != 0;

-- Phase 2: Insert reading_progress rows that have no status row yet.
INSERT OR IGNORE INTO reading_state (fingerprint, status, position, percentage, last_updated, status_updated_at)
SELECT fingerprint, 0, progress, 0.0, last_updated, '1970-01-01T00:00:00Z'
FROM reading_progress;

-- Phase 3: Back-fill position/last_updated for rows inserted from status migration
-- that also have a progress record.
UPDATE reading_state
SET position     = (SELECT progress      FROM reading_progress rp WHERE rp.fingerprint = reading_state.fingerprint),
    last_updated = (SELECT rp.last_updated FROM reading_progress rp WHERE rp.fingerprint = reading_state.fingerprint)
WHERE fingerprint IN (SELECT fingerprint FROM reading_progress)
  AND status != 0;

DROP TABLE reading_progress;

-- Remove status column from contents (SQLite requires table recreation to drop a column).
CREATE TABLE contents_new (
    fingerprint TEXT    NOT NULL PRIMARY KEY,
    document_id INTEGER REFERENCES documents (id) ON DELETE SET NULL
);
INSERT INTO contents_new (fingerprint, document_id)
SELECT fingerprint, document_id FROM contents;
DROP TABLE contents;
ALTER TABLE contents_new RENAME TO contents;

PRAGMA foreign_keys = ON;
