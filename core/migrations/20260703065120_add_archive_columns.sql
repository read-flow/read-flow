-- Files inside archives (zip, tar): archive_path points at the archive on disk,
-- archive_inner_path is the member path inside the archive. Both NULL for
-- regular files. files.path stays UNIQUE using the synthetic form
-- "{archive_path}::{archive_inner_path}" for archive members.
ALTER TABLE files
    ADD COLUMN archive_path TEXT DEFAULT NULL;
ALTER TABLE files
    ADD COLUMN archive_inner_path TEXT DEFAULT NULL;
