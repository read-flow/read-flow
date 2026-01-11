-- Add authorization_token column to remotes table
ALTER TABLE remotes ADD COLUMN authorization_token VARCHAR NOT NULL DEFAULT 'secret';
