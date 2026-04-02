CREATE TABLE remotes (
       id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
       base_url VARCHAR NOT NULL,
       "order" INTEGER NOT NULL DEFAULT 0,
       passphrase VARCHAR NOT NULL DEFAULT 'secret',
       user_id TEXT NOT NULL DEFAULT 'default_user'
);

CREATE UNIQUE INDEX uq_remote_base_url ON remotes (base_url);

CREATE UNIQUE INDEX uq_remote_order ON remotes ("order");
