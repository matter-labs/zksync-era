DROP INDEX IF EXISTS idx_tee_proof_generation_details_status_prover_taken_at;

CREATE TYPE tee_proofs_job_status AS ENUM ('Generated', 'PickedByProver', 'Unpicked');

ALTER TABLE tee_proof_generation_details
    ALTER COLUMN status TYPE tee_proofs_job_status
    USING CASE
        WHEN LOWER(status) = 'generated' THEN 'Generated'::tee_proofs_job_status
        WHEN LOWER(status) = 'picked_by_prover' THEN 'PickedByProver'::tee_proofs_job_status
        WHEN LOWER(status) IN ('ready_to_be_proven', 'unpicked') THEN 'Unpicked'::tee_proofs_job_status
    END,
    ALTER COLUMN status SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_tee_proof_generation_details_status_prover_taken_at
    ON tee_proof_generation_details (prover_taken_at)
    WHERE status = 'PickedByProver';
