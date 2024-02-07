ALTER TABLE blocks ADD COLUMN l2_l1_compressed_messages BYTEA;
ALTER TABLE blocks ADD COLUMN compressed_write_logs BYTEA;
ALTER TABLE blocks ADD COLUMN compressed_contracts BYTEA;
ALTER TABLE blocks ADD COLUMN parent_hash BYTEA;
