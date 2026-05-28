CREATE TABLE IF NOT EXISTS covers (
    fingerprint TEXT PRIMARY KEY
        REFERENCES contents (fingerprint) ON DELETE CASCADE,
    data        BLOB NOT NULL,
    mime        TEXT NOT NULL DEFAULT 'image/jpeg'
);
