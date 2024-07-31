CREATE TYPE tee_proofs_job_status AS ENUM ('ReadyToBeProven', 'PickedByProver', 'Generated', 'Skipped');

CREATE TABLE IF NOT EXISTS tee_attestations
(
    pubkey                  BYTEA PRIMARY KEY,
    attestation             BYTEA
);

CREATE TABLE IF NOT EXISTS tee_proofs
(
    l1_batch_number         BIGINT NOT NULL REFERENCES tee_verifier_input_producer_jobs (l1_batch_number) ON DELETE CASCADE,
    tee_type                TEXT NOT NULL,
    status                  tee_proofs_job_status NOT NULL,
    pubkey                  BYTEA REFERENCES tee_attestations (pubkey) ON DELETE CASCADE,
    signature               BYTEA,
    proof                   BYTEA,
    created_at              TIMESTAMP NOT NULL,
    updated_at              TIMESTAMP NOT NULL,
    prover_taken_at         TIMESTAMP,
    PRIMARY KEY (l1_batch_number, tee_type)
);

CREATE INDEX IF NOT EXISTS idx_tee_proofs_status_prover_taken_at
    ON tee_proofs (prover_taken_at)
    WHERE status = 'PickedByProver';
