ALTER TABLE eth_proof_manager
    ALTER COLUMN witness_inputs_url DROP NOT NULL;

ALTER TABLE eth_proof_manager
    ADD COLUMN submission_started_at TIMESTAMP;
