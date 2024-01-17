ALTER TABLE snapshot_recovery
    RENAME COLUMN miniblock_hash TO miniblock_root_hash;
ALTER TABLE snapshot_recovery
    DROP COLUMN l1_batch_timestamp,
    DROP COLUMN miniblock_timestamp,
    DROP COLUMN protocol_version;
