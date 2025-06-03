ALTER TABLE miniblocks ADD COLUMN rolling_txs_hash BYTEA;
ALTER TABLE miniblocks ADD COLUMN eth_precommit_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL;
ALTER TABLE eth_txs ADD COLUMN status TEXT;
ALTER TABLE l1_batches ADD COLUMN final_precommit_eth_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS transactions_l1_batch ON transactions (l1_batch_number) WHERE l1_batch_number IS NOT NULL;
