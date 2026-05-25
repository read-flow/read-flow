CREATE TABLE documents (
    id   INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    guid TEXT    NOT NULL UNIQUE
);

CREATE TABLE contents (
    fingerprint TEXT    NOT NULL PRIMARY KEY,
    status      INTEGER NOT NULL DEFAULT 0,
    document_id INTEGER REFERENCES documents (id) ON DELETE SET NULL
);

CREATE TABLE content_tags (
    fingerprint TEXT NOT NULL REFERENCES contents (fingerprint) ON DELETE CASCADE,
    tag         TEXT NOT NULL,
    PRIMARY KEY (fingerprint, tag)
);
CREATE INDEX idx_content_tags_tag ON content_tags (tag);

CREATE TABLE files (
    id          INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    guid        TEXT    NOT NULL UNIQUE,
    path        TEXT    NOT NULL UNIQUE,
    type        TEXT    NOT NULL,
    size        INTEGER NOT NULL,
    fingerprint TEXT    NOT NULL REFERENCES contents (fingerprint)
);
CREATE UNIQUE INDEX uq_file_guid  ON files (guid);
CREATE UNIQUE INDEX uq_file_path  ON files (path);
CREATE        INDEX idx_file_fp   ON files (fingerprint);
CREATE        INDEX idx_file_type ON files (type);

CREATE TABLE remotes (
    id         INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    base_url   VARCHAR NOT NULL,
    "order"    INTEGER NOT NULL DEFAULT 0,
    passphrase VARCHAR NOT NULL DEFAULT 'secret',
    user_id    TEXT    NOT NULL DEFAULT 'default_user'
);
CREATE UNIQUE INDEX uq_remote_base_url ON remotes (base_url);
CREATE UNIQUE INDEX uq_remote_order    ON remotes ("order");

CREATE TABLE reading_progress (
    fingerprint  TEXT NOT NULL PRIMARY KEY,
    progress     TEXT NOT NULL DEFAULT '{}',
    last_updated TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z'
);

CREATE TABLE document_metadata (
    document_id   INTEGER PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    document_type TEXT,
    title         TEXT,
    subtitle      TEXT,
    authors       TEXT,
    description   TEXT,
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    language      TEXT,
    publisher     TEXT,
    identifier    TEXT,
    date          TEXT,
    subject       TEXT
);
