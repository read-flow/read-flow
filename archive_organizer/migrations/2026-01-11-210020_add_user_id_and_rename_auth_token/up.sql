-- Add user_id column to remotes table
ALTER TABLE remotes ADD COLUMN user_id TEXT NOT NULL DEFAULT 'default_user';

-- Rename authorization_token column to passphrase
ALTER TABLE remotes RENAME COLUMN authorization_token TO passphrase;
