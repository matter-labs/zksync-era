ALTER TABLE miniblocks ADD COLUMN rolling_txs_hash BYTEA;
ALTER TABLE miniblocks ADD COLUMN eth_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL;
ALTER TABLE eth_txs ADD COLUMN status TEXT;
