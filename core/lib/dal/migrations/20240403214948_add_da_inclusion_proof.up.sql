ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS da_inclusion_proof BYTEA[] NOT NULL DEFAULT '{}';
