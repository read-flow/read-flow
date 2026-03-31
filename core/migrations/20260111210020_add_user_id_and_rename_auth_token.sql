ALTER TABLE remotes ADD COLUMN user_id TEXT NOT NULL DEFAULT 'default_user';

ALTER TABLE remotes RENAME COLUMN authorization_token TO passphrase;
