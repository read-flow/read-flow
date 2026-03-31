CREATE TABLE remotes (
       id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       base_url VARCHAR NOT NULL
);

CREATE UNIQUE INDEX uq_remote_base_url ON remotes (base_url);
