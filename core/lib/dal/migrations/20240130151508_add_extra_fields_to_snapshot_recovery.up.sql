ALTER TABLE snapshot_recovery
    ADD COLUMN miniblock_hash BYTEA NOT NULL,
    ADD COLUMN l1_batch_timestamp BIGINT NOT NULL,
    ADD COLUMN miniblock_timestamp BIGINT NOT NULL,
    ADD COLUMN protocol_version INT NOT NULL;
-- `miniblock_root_hash` should be renamed to `miniblock_hash`, but we cannot do it straightforwardly
-- because of backward compatibility. Instead, we create a new column and set a dummy default value
-- for the old one, so that INSERTs not referencing `miniblock_root_hash` don't fail.
ALTER TABLE snapshot_recovery
    ALTER COLUMN miniblock_root_hash SET DEFAULT '\x0000000000000000000000000000000000000000000000000000000000000000'::bytea;
