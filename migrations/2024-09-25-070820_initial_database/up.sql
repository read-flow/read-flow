-- Your SQL goes here
CREATE TABLE files (
       id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       path VARCHAR NOT NULL,
       type VARCHAR NOT NULL,
       size INTEGER NOT NULL,
       sha256sum VARCHAR NOT NULL
);

CREATE UNIQUE INDEX uq_file_path ON files (path);

CREATE INDEX idx_file_size_hash ON files (size, sha256sum);

CREATE INDEX idx_file_type ON files (type);

CREATE TABLE file_tags (
       file_id INTEGER NOT NULL,
       tag VARCHAR NOT NULL,

       PRIMARY KEY(file_id, tag)
);

CREATE TABLE directories (
       id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       path VARCHAR NOT NULL,
       type VARCHAR NOT NULL
);

CREATE UNIQUE INDEX uq_directory_path ON directories (path);

CREATE INDEX idx_directory_type ON directories (type);

CREATE TABLE directory_tags (
       directory_id INTEGER NOT NULL,
       tag VARCHAR NOT NULL,

       PRIMARY KEY(directory_id, tag)
);
