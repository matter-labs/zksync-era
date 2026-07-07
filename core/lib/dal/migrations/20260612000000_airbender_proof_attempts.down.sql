ALTER TABLE airbender_proof_generation_details
    DROP COLUMN IF EXISTS attempts,
    DROP COLUMN IF EXISTS snark_attempts,
    DROP COLUMN IF EXISTS error;
