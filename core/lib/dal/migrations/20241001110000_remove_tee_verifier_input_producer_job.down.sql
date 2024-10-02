CREATE TABLE tee_proof_generation_details (
    l1_batch_number BIGINT NOT NULL,
    status TEXT NOT NULL,
    signature BYTEA,
    pubkey BYTEA,
    proof BYTEA,
    tee_type TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    prover_taken_at TIMESTAMP,
    PRIMARY KEY (l1_batch_number, tee_type),
    CONSTRAINT tee_proof_generation_details_l1_batch_number_fkey FOREIGN KEY (l1_batch_number) REFERENCES tee_verifier_input_producer_jobs(l1_batch_number) ON DELETE CASCADE,
    CONSTRAINT tee_proof_generation_details_pubkey_fkey FOREIGN KEY (pubkey) REFERENCES tee_attestations(pubkey) ON DELETE SET NULL
);

CREATE INDEX idx_tee_proof_generation_details_status_prover_taken_at
    ON tee_proof_generation_details (prover_taken_at)
    WHERE status = 'picked_by_prover';
