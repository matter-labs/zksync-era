DROP INDEX IF EXISTS idx_tee_proofs_status_prover_taken_at;
ALTER TABLE tee_proofs RENAME TO tee_proof_generation_details;
ALTER TABLE tee_proof_generation_details DROP CONSTRAINT tee_proof_generation_details_pkey;
ALTER TABLE tee_proof_generation_details DROP CONSTRAINT tee_proof_generation_details_pubkey_fkey;
ALTER TABLE tee_proof_generation_details
    ADD CONSTRAINT tee_proof_generation_details_pubkey_fkey
    FOREIGN KEY (pubkey) REFERENCES tee_attestations (pubkey) ON DELETE SET NULL;
ALTER TABLE tee_proof_generation_details
    ALTER COLUMN status TYPE TEXT
    USING CASE
        WHEN status = 'ReadyToBeProven' THEN 'ready_to_be_proven'
        WHEN status = 'PickedByProver' THEN 'picked_by_prover'
        WHEN status = 'Generated' THEN 'generated'
        WHEN status = 'Skipped' THEN 'skipped'
    END,
    ALTER COLUMN status DROP NOT NULL;
ALTER TABLE tee_proof_generation_details ALTER COLUMN tee_type DROP NOT NULL;
ALTER TABLE tee_proof_generation_details ALTER COLUMN l1_batch_number DROP NOT NULL;
ALTER TABLE tee_proof_generation_details ADD PRIMARY KEY (l1_batch_number);
DROP TYPE IF EXISTS tee_proofs_job_status;
CREATE INDEX IF NOT EXISTS idx_tee_proof_generation_details_status_prover_taken_at
    ON tee_proof_generation_details (status, prover_taken_at);
