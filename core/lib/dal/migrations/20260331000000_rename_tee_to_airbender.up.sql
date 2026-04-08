CREATE TABLE IF NOT EXISTS airbender_proof_generation_details
(
    l1_batch_number         BIGINT NOT NULL PRIMARY KEY,
    status                  TEXT NOT NULL,
    proof_blob_url          TEXT,
    prover_id               TEXT,
    created_at              TIMESTAMP NOT NULL,
    updated_at              TIMESTAMP NOT NULL,
    prover_taken_at         TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_airbender_proof_generation_details_status_prover_taken_at
    ON airbender_proof_generation_details (prover_taken_at)
    WHERE status = 'picked_by_prover';
