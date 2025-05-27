-- Drop indexes
DROP INDEX IF EXISTS idx_api_keys_key;
DROP INDEX IF EXISTS idx_api_keys_user_id;

-- Drop tables
DROP TABLE IF EXISTS api_keys;
DROP TABLE IF EXISTS users;
