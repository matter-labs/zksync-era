ALTER TABLE l1_batches
    DROP COLUMN IF EXISTS system_logs;

ALTER TABLE l1_batches
    DROP COLUMN IF EXISTS compressed_state_diffs;
