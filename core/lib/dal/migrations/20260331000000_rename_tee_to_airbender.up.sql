CREATE TABLE IF NOT EXISTS airbender_attestations
(
    pubkey                  BYTEA PRIMARY KEY,
    attestation             BYTEA
);

CREATE TABLE IF NOT EXISTS airbender_proof_generation_details
(
    l1_batch_number         BIGINT NOT NULL,
    status                  TEXT NOT NULL,
    signature               BYTEA,
    pubkey                  BYTEA REFERENCES airbender_attestations (pubkey) ON DELETE SET NULL,
    proof                   BYTEA,
    tee_type                TEXT NOT NULL,
    created_at              TIMESTAMP NOT NULL,
    updated_at              TIMESTAMP NOT NULL,
    prover_taken_at         TIMESTAMP,
    PRIMARY KEY (l1_batch_number, tee_type)
);

CREATE INDEX IF NOT EXISTS idx_airbender_proof_generation_details_status_prover_taken_at
    ON airbender_proof_generation_details (prover_taken_at)
    WHERE status = 'picked_by_prover';
