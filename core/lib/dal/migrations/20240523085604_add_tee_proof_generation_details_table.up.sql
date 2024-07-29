CREATE TABLE IF NOT EXISTS tee_attestations
(
    pubkey                  BYTEA PRIMARY KEY,
    attestation             BYTEA
);

CREATE TABLE IF NOT EXISTS tee_proofs
(
    id                      BIGSERIAL PRIMARY KEY,
    l1_batch_number         BIGINT NOT NULL REFERENCES tee_verifier_input_producer_jobs (l1_batch_number) ON DELETE CASCADE,
    tee_type                TEXT NOT NULL,
    pubkey                  BYTEA REFERENCES tee_attestations (pubkey) ON DELETE CASCADE,
    signature               BYTEA,
    proof                   BYTEA,
    created_at              TIMESTAMP NOT NULL,
    proved_at               TIMESTAMP,
    prover_taken_at         TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_proofs_number_per_batch_number_and_tee_type
    ON tee_proofs (l1_batch_number, tee_type);
