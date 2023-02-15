ALTER TABLE eth_txs_history DROP COLUMN signed_raw_tx;
ALTER table eth_txs ALTER COLUMN sent_at_block SET NOT NULL;
ALTER table eth_txs_history DROP COLUMN sent_at_block;
ALTER table eth_txs_history DROP COLUMN sent_at;
