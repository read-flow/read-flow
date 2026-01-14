-- This file should undo anything in `up.sql`

-- Rename passphrase column back to authorization_token
ALTER TABLE remotes RENAME COLUMN passphrase TO authorization_token;

-- Remove user_id column from remotes table
ALTER TABLE remotes DROP COLUMN user_id;
