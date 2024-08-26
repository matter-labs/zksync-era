DROP INDEX IF EXISTS idx_tee_proof_generation_details_status_prover_taken_at;

ALTER TABLE tee_proof_generation_details
    ALTER COLUMN status TYPE TEXT
    USING CASE
        WHEN status = 'Generated' THEN 'generated'
        WHEN status = 'PickedByProver' THEN 'picked_by_prover'
        WHEN status = 'Unpicked' THEN 'ready_to_be_proven'
    END,
    ALTER COLUMN status DROP NOT NULL;

DROP TYPE IF EXISTS tee_proofs_job_status;

CREATE INDEX IF NOT EXISTS idx_tee_proof_generation_details_status_prover_taken_at
    ON tee_proof_generation_details (prover_taken_at)
    WHERE status = 'picked_by_prover';
