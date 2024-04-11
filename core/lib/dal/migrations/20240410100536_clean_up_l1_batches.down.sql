ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS gas_per_pubdata_byte_in_block INT;
ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS gas_per_pubdata_limit BIGINT NOT NULL DEFAULT 0;
ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS compressed_write_logs BYTEA;
ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS compressed_contracts BYTEA;
ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS parent_hash BYTEA;
