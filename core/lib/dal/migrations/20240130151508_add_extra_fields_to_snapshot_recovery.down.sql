ALTER TABLE snapshot_recovery
    DROP COLUMN miniblock_hash,
    DROP COLUMN l1_batch_timestamp,
    DROP COLUMN miniblock_timestamp,
    DROP COLUMN protocol_version;
