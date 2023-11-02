ALTER TABLE eth_txs_history ADD COLUMN signed_raw_tx BYTEA;
-- Deprecated column
ALTER table eth_txs ALTER COLUMN sent_at_block DROP NOT NULL;
ALTER table eth_txs_history ADD COLUMN sent_at_block INT;
ALTER table eth_txs_history ADD COLUMN sent_at TIMESTAMP;
