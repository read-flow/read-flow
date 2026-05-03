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

CREATE TABLE content_metadata (
    fingerprint  TEXT NOT NULL PRIMARY KEY
                     REFERENCES contents (fingerprint) ON DELETE CASCADE,
    title        TEXT,
    authors      TEXT,
    language     TEXT,
    publisher    TEXT,
    identifier   TEXT,
    date         TEXT,
    extracted_at TEXT NOT NULL
);

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
