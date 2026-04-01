CREATE TABLE files (
       id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       path VARCHAR NOT NULL,
       type VARCHAR NOT NULL,
       size INTEGER NOT NULL,
       fingerprint VARCHAR NOT NULL,
       status INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX uq_file_path ON files (path);

CREATE INDEX idx_file_size_hash ON files (size, fingerprint);

CREATE INDEX idx_file_type ON files (type);

CREATE TABLE file_tags (
       file_id INTEGER NOT NULL REFERENCES files (id),
       tag VARCHAR NOT NULL,

       PRIMARY KEY(file_id, tag)
);

CREATE INDEX idx_file_tags_tag ON file_tags (tag);
