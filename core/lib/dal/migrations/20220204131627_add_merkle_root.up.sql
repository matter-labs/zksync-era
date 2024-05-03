ALTER TABLE blocks ADD COLUMN merkle_root_hash BYTEA;
UPDATE blocks SET merkle_root_hash = hash;
