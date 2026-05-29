ALTER TABLE document_metadata
ADD COLUMN selected_cover_fingerprint TEXT REFERENCES contents (fingerprint) ON DELETE SET NULL;
