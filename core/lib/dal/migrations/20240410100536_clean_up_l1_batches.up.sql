ALTER TABLE l1_batches DROP COLUMN IF EXISTS gas_per_pubdata_byte_in_block;
ALTER TABLE l1_batches DROP COLUMN IF EXISTS gas_per_pubdata_limit;
ALTER TABLE l1_batches DROP COLUMN IF EXISTS compressed_write_logs;
ALTER TABLE l1_batches DROP COLUMN IF EXISTS compressed_contracts;
ALTER TABLE l1_batches DROP COLUMN IF EXISTS parent_hash;
