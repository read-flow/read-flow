-- SQLite rejects non-constant defaults (e.g. strftime(...)) in ALTER TABLE ADD COLUMN,
-- so add with a constant default first, then backfill existing rows. Every row ends
-- this migration with a real timestamp; the empty string never persists past it.
ALTER TABLE files ADD COLUMN imported_at TEXT NOT NULL DEFAULT '';
UPDATE files SET imported_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE imported_at = '';
