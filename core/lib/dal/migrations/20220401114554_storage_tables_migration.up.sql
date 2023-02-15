ALTER TABLE storage RENAME COLUMN raw_key TO hashed_key;

DROP INDEX storage_logs_raw_key_block_number_idx;
ALTER TABLE storage_logs DROP CONSTRAINT storage_logs_pkey;
ALTER TABLE storage_logs DROP COLUMN id;
ALTER TABLE storage_logs RENAME COLUMN raw_key TO hashed_key;
ALTER TABLE storage_logs ADD PRIMARY KEY (hashed_key, block_number, operation_number);
