ALTER TABLE miniblocks ADD COLUMN rolling_txs_hash BYTEA;
ALTER TABLE miniblocks ADD COLUMN eth_precommit_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL;
ALTER TABLE eth_txs ADD COLUMN status TEXT;
ALTER TABLE l1_batches ADD COLUMN final_precommit_eth_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL;
