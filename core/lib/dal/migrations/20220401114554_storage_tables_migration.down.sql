ALTER TABLE storage RENAME COLUMN hashed_key TO raw_key;

ALTER TABLE storage_logs DROP CONSTRAINT storage_logs_pkey;
ALTER TABLE storage_logs RENAME COLUMN hashed_key TO raw_key;
ALTER TABLE storage_logs ADD COLUMN id SERIAL PRIMARY KEY;
CREATE INDEX storage_logs_raw_key_block_number_idx ON storage_logs (raw_key, block_number DESC, operation_number DESC);
