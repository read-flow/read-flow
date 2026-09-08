CREATE TABLE audit_operations (
    id            TEXT PRIMARY KEY NOT NULL,
    operation_type TEXT NOT NULL,
    actor_kind    TEXT NOT NULL,
    actor_id      TEXT,
    channel       TEXT NOT NULL,
    status        TEXT NOT NULL,
    dry_run       INTEGER NOT NULL DEFAULT 0,
    started_at    TEXT NOT NULL,
    completed_at  TEXT,
    error_code    TEXT,
    counts_json   TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE audit_events (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id   TEXT NOT NULL REFERENCES audit_operations (id) ON DELETE CASCADE,
    event_type     TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    sequence       INTEGER NOT NULL,
    occurred_at    TEXT NOT NULL,
    outcome        TEXT NOT NULL,
    parameters_json TEXT NOT NULL,
    UNIQUE (operation_id, sequence)
);

CREATE TABLE audit_targets (
    event_id              INTEGER NOT NULL REFERENCES audit_events (id) ON DELETE CASCADE,
    target_kind           TEXT NOT NULL,
    target_id             TEXT NOT NULL,
    title_snapshot        TEXT,
    path_snapshot         TEXT,
    fingerprint_snapshot  TEXT,
    PRIMARY KEY (event_id, target_kind, target_id)
);

CREATE INDEX idx_audit_operations_started_at
    ON audit_operations (started_at DESC, id DESC);
CREATE INDEX idx_audit_operations_actor_started_at
    ON audit_operations (actor_id, started_at DESC, id DESC);
CREATE INDEX idx_audit_operations_type_started_at
    ON audit_operations (operation_type, started_at DESC, id DESC);
CREATE INDEX idx_audit_events_operation_sequence
    ON audit_events (operation_id, sequence);
CREATE INDEX idx_audit_targets_target
    ON audit_targets (target_kind, target_id, event_id);
