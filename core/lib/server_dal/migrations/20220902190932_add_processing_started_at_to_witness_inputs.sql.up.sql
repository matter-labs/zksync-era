ALTER TABLE witness_inputs
    ADD COLUMN IF NOT EXISTS processing_started_at TIMESTAMP;
