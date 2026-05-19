-- Add columns to track Airbender SNARK proof generation alongside the existing FRI proof.

ALTER TABLE airbender_proof_generation_details
    ADD COLUMN IF NOT EXISTS snark_proof_blob_url TEXT,
    ADD COLUMN IF NOT EXISTS snark_prover_id TEXT,
    ADD COLUMN IF NOT EXISTS snark_taken_at TIMESTAMP;

CREATE INDEX IF NOT EXISTS idx_airbender_proof_generation_details_snark_status_taken_at
    ON airbender_proof_generation_details (snark_taken_at)
    WHERE status = 'picked_for_snark';
