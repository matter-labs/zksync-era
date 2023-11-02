ALTER TABLE eth_txs DROP COLUMN gas_price;
ALTER TABLE eth_txs DROP COLUMN priority_fee_per_gas;
ALTER TABLE eth_txs DROP COLUMN base_fee_per_gas;
ALTER TABLE eth_txs DROP COLUMN confirmed_tx_hash;
ALTER TABLE eth_txs DROP COLUMN confirmed_at;
ALTER TABLE eth_txs ADD COLUMN sent_at_block INT NOT NULL;

ALTER TABLE eth_txs ADD COLUMN confirmed_eth_tx_history_id INT REFERENCES eth_txs_history(id) ON DELETE SET NULL;
CREATE INDEX inflight_eth_txs ON eth_txs ((confirmed_eth_tx_history_id IS NULL));

ALTER TABLE eth_txs_history ALTER COLUMN priority_fee_per_gas SET NOT NULL;
ALTER TABLE eth_txs_history ALTER COLUMN base_fee_per_gas SET NOT NULL;
ALTER TABLE eth_txs_history DROP COLUMN deadline_block;
ALTER TABLE eth_txs_history DROP COLUMN error;
ALTER TABLE eth_txs_history DROP COLUMN gas_price;
ALTER TABLE eth_txs_history ADD COLUMN confirmed_at TIMESTAMP;

