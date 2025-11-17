-- This file should undo anything in `up.sql`
DROP INDEX uq_remote_order;

ALTER TABLE remotes
DROP COLUMN 'order';
