ALTER TABLE tee_attestations
    ADD COLUMN eth_tx_id INTEGER REFERENCES eth_txs (id) ON DELETE SET NULL;
ALTER TABLE tee_attestations
    ADD COLUMN calldata BYTEA;
ALTER TABLE tee_attestations
    ADD COLUMN created_at TIMESTAMPTZ;
ALTER TABLE tee_proof_generation_details
    ADD COLUMN eth_tx_id INTEGER REFERENCES eth_txs (id) ON DELETE SET NULL;
ALTER TABLE tee_proof_generation_details
    ADD COLUMN calldata BYTEA;
