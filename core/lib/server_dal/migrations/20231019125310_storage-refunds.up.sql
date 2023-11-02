ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS storage_refunds BIGINT[];
