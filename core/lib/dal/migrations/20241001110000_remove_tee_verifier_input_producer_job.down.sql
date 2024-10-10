CREATE TABLE tee_verifier_input_producer_jobs (
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

ALTER TABLE tee_proof_generation_details
    ADD CONSTRAINT tee_proof_generation_details_l1_batch_number_fkey
    FOREIGN KEY (l1_batch_number)
    REFERENCES tee_verifier_input_producer_jobs(l1_batch_number)
    ON DELETE CASCADE;
