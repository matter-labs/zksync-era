CREATE TABLE IF NOT EXISTS eth_proof_manager (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
    assigned_to TEXT,
    status TEXT NOT NULL,
    witness_inputs_url TEXT NOT NULL,
    proof_blob_url TEXT NOT NULL,
    submit_proof_request_tx_hash BYTEA DEFAULT NULL,
    submit_proof_request_attempts INT NOT NULL DEFAULT 0,
    submit_proof_request_tx_sent_at TIMESTAMP DEFAULT NULL,
    validated_proof_request_tx_hash BYTEA DEFAULT NULL,
    validated_proof_request_attempts INT NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
);
