CREATE TABLE IF NOT EXISTS eth_proof_manager (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY REFERENCES l1_batches(number) ON DELETE CASCADE,
    status TEXT NOT NULL,
    proof_gen_data_blob_url TEXT NOT NULL,
    proof_blob_url TEXT,
    assigned_to BIGINT,
    validation_status TEXT,
    sent_proof_request_tx_hash TEXT,
    sent_proof_validation_result_tx_hash TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
