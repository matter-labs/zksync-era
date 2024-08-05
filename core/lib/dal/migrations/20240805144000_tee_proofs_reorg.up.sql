DROP INDEX IF EXISTS idx_tee_proof_generation_details_status_prover_taken_at;
CREATE TYPE tee_proofs_job_status AS ENUM ('ReadyToBeProven', 'PickedByProver', 'Generated', 'Skipped');
ALTER TABLE tee_proof_generation_details DROP CONSTRAINT tee_proof_generation_details_pkey;
UPDATE tee_proof_generation_details SET tee_type = 'sgx' WHERE tee_type IS NULL;
ALTER TABLE tee_proof_generation_details ALTER COLUMN l1_batch_number SET NOT NULL;
ALTER TABLE tee_proof_generation_details ALTER COLUMN tee_type SET NOT NULL;
ALTER TABLE tee_proof_generation_details
    ALTER COLUMN status TYPE tee_proofs_job_status
    USING CASE
        WHEN status = 'ready_to_be_proven' THEN 'ReadyToBeProven'::tee_proofs_job_status
        WHEN status = 'picked_by_prover' THEN 'PickedByProver'::tee_proofs_job_status
        WHEN status = 'generated' THEN 'Generated'::tee_proofs_job_status
        WHEN status = 'skipped' THEN 'Skipped'::tee_proofs_job_status
        ELSE
            RAISE EXCEPTION 'Unexpected value for status: %', status
    END,
    ALTER COLUMN status SET NOT NULL;
ALTER TABLE tee_proof_generation_details DROP CONSTRAINT IF EXISTS tee_proof_generation_details_pubkey_fkey;
ALTER TABLE tee_proof_generation_details
    ADD CONSTRAINT tee_proof_generation_details_pubkey_fkey
    FOREIGN KEY (pubkey) REFERENCES tee_attestations (pubkey) ON DELETE CASCADE;
ALTER TABLE tee_proof_generation_details ADD PRIMARY KEY (l1_batch_number, tee_type);
ALTER TABLE tee_proof_generation_details RENAME TO tee_proofs;
CREATE INDEX IF NOT EXISTS idx_tee_proofs_status_prover_taken_at
    ON tee_proofs (prover_taken_at)
    WHERE status = 'PickedByProver';
