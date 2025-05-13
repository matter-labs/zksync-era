-- Add up migration script here
CREATE TABLE rolling_tx_hashes (
    id SERIAL NOT NULL PRIMARY KEY,
		rolling_hash  BYTEA,
		l1_batch_number BIGINT,
		final BOOLEAN,
		eth_tx_id INT REFERENCES eth_txs (id) ON DELETE SET NULL -- tx submitting this rolling hash
);

ALTER TABLE miniblocks ADD COLUMN rolling_txs_id INT;
--
ALTER TABLE rolling_tx_hashes ADD CONSTRAINT rolling_txs_l1_batch_id FOREIGN KEY (l1_batch_number) REFERENCES l1_batches(number) ON DELETE CASCADE;

ALTER TABLE miniblocks ADD CONSTRAINT rolling_txs_transactions FOREIGN KEY (rolling_txs_id) REFERENCES rolling_tx_hashes(id) ON DELETE SET NULL;

ALTER TABLE l1_batches ADD COLUMN finality TEXT;
CREATE TABLE finality  (
   id SERIAL NOT NULL PRIMARY KEY,
   precommit_tx_hash BYTEA,
   gateway_l1_batch_number BIGINT,
   execute_tx_hash BYTEA
);

ALTER TABLE eth_txs ADD COLUMN finality_id INT;
