ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS da_inclusion_data BYTEA[] NOT NULL DEFAULT '{}';
