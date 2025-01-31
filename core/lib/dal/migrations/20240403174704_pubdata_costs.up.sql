ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS pubdata_costs BIGINT[];
