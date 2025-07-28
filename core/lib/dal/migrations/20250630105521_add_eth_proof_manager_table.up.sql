CREATE TABLE IF NOT EXISTS proof_manager (
    l1_batch_number BIGINT NOT NULL PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
    assigned_to TEXT,
    status TEXT NOT NULL,
    witness_inputs_url TEXT NOT NULL,
    proof_validation_result BOOLEAN,
    submit_proof_request_tx_hash BYTEA,
    submit_proof_request_tx_sent_at TIMESTAMP,
    validated_proof_request_tx_hash BYTEA,
    validated_proof_request_tx_sent_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE TYPE proving_mode AS ENUM ('proving_network', 'prover_cluster');
ALTER TABLE proof_generation_details ADD COLUMN proving_mode proving_mode DEFAULT 'proving_network';

ALTER TYPE event_type ADD VALUE 'ProofRequestAcknowledged';
ALTER TYPE event_type ADD VALUE 'ProofRequestProven';
