CREATE TABLE reading_progress (
    fingerprint TEXT NOT NULL PRIMARY KEY,
    progress TEXT NOT NULL DEFAULT '{}',
    last_updated TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z'
);
