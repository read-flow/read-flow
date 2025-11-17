-- Your SQL goes here
ALTER TABLE remotes
ADD COLUMN 'order' INTEGER NOT NULL DEFAULT 0;

UPDATE remotes
SET "order" = updated_values.new_order - 1
FROM (SELECT rowid, ROW_NUMBER() OVER (ORDER BY id) AS new_order FROM remotes) AS updated_values
WHERE remotes.rowid = updated_values.rowid;

CREATE UNIQUE INDEX uq_remote_order ON remotes ('order');
