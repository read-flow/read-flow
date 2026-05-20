-- Extend document_metadata with the fields from content_metadata.
ALTER TABLE document_metadata ADD COLUMN language    TEXT;
ALTER TABLE document_metadata ADD COLUMN publisher   TEXT;
ALTER TABLE document_metadata ADD COLUMN identifier  TEXT;
ALTER TABLE document_metadata ADD COLUMN date        TEXT;
ALTER TABLE document_metadata ADD COLUMN subject     TEXT;

-- For ungrouped contents that have metadata: create a documents row per
-- content fingerprint (using the fingerprint as the guid) and link the content.
INSERT OR IGNORE INTO documents (guid)
SELECT cm.fingerprint
FROM content_metadata cm
JOIN contents c ON cm.fingerprint = c.fingerprint
WHERE c.document_id IS NULL;

UPDATE contents
SET document_id = (SELECT id FROM documents WHERE guid = contents.fingerprint)
WHERE document_id IS NULL
  AND fingerprint IN (SELECT fingerprint FROM content_metadata);

-- Migrate content_metadata rows → document_metadata.
-- INSERT OR IGNORE preserves any metadata the user has already edited.
-- Authors: content_metadata stored a comma-separated string; wrap it in a
-- single-element JSON array here — re-scanning will split and extend properly.
INSERT OR IGNORE INTO document_metadata
    (document_id, title, authors, language, publisher, identifier, date, subject)
SELECT
    c.document_id,
    cm.title,
    CASE WHEN cm.authors IS NOT NULL AND TRIM(cm.authors) != ''
         THEN '["' || REPLACE(TRIM(cm.authors), '"', '\"') || '"]'
         ELSE NULL
    END,
    cm.language,
    cm.publisher,
    cm.identifier,
    cm.date,
    cm.subject
FROM content_metadata cm
JOIN contents c ON cm.fingerprint = c.fingerprint
WHERE c.document_id IS NOT NULL
  AND (cm.title IS NOT NULL OR cm.authors IS NOT NULL
       OR cm.language IS NOT NULL OR cm.publisher IS NOT NULL);

-- Drop the old table.
DROP TABLE content_metadata;
