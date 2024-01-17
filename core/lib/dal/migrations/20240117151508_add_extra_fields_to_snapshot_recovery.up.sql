ALTER TABLE snapshot_recovery
    RENAME COLUMN miniblock_root_hash TO miniblock_hash;
ALTER TABLE snapshot_recovery
    ADD COLUMN l1_batch_timestamp BIGINT NOT NULL,
    ADD COLUMN miniblock_timestamp BIGINT NOT NULL,
    ADD COLUMN protocol_version INT NOT NULL;
