ALTER TABLE eth_txs ADD COLUMN gas_price BIGINT;
ALTER TABLE eth_txs ADD COLUMN priority_fee_per_gas BIGINT;
ALTER TABLE eth_txs ADD COLUMN base_fee_per_gas BIGINT;
ALTER TABLE eth_txs ADD COLUMN confirmed_tx_hash TEXT;
ALTER TABLE eth_txs ADD COLUMN confirmed_at TIMESTAMP NOT NULL;
ALTER TABLE eth_txs DROP COLUMN sent_at_block;

ALTER TABLE eth_txs DROP COLUMN confirmed_eth_tx_history_id;

ALTER TABLE eth_txs_history ALTER COLUMN priority_fee_per_gas DROP NOT NULL;
ALTER TABLE eth_txs_history ALTER COLUMN base_fee_per_gas DROP NOT NULL;
ALTER TABLE eth_txs_history ADD COLUMN deadline_block INT;
ALTER TABLE eth_txs_history ADD COLUMN error TEXT;
ALTER TABLE eth_txs_history ADD COLUMN gas_price BIGINT;
ALTER TABLE eth_txs_history DROP COLUMN confirmed_at;

