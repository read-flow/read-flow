CREATE TABLE document_metadata (
    document_id   INTEGER PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    document_type TEXT,
    title         TEXT,
    subtitle      TEXT,
    authors       TEXT,    -- JSON array, e.g. '["Knuth","Turing"]'
    description   TEXT,
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
