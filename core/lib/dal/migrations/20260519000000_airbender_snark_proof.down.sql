DROP INDEX IF EXISTS idx_airbender_proof_generation_details_snark_status_taken_at;
DROP INDEX IF EXISTS idx_airbender_proof_generation_details_snark_ready;

ALTER TABLE airbender_proof_generation_details
    DROP COLUMN IF EXISTS snark_taken_at,
    DROP COLUMN IF EXISTS snark_prover_id,
    DROP COLUMN IF EXISTS snark_proof_blob_url;
