ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS system_logs BYTEA[]
    NOT NULL DEFAULT '{}';

ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS compressed_state_diffs BYTEA;
