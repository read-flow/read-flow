-- This file should undo anything in `up.sql`
DROP INDEX idx_directory_tags_tag;

DROP TABLE directory_tags;

DROP INDEX idx_directory_type;

DROP INDEX uq_directory_path;

DROP TABLE directories;

DROP INDEX idx_file_tags_tag;

DROP TABLE file_tags;

DROP INDEX idx_file_type;

DROP INDEX idx_file_size_hash;

DROP INDEX uq_file_path;

DROP TABLE files;
