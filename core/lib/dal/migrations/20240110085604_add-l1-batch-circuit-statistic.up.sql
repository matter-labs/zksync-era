ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS predicted_circuits_by_type JSONB;
