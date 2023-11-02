ALTER TABLE eth_txs ADD COLUMN base_fee_per_gas BIGINT;
ALTER TABLE eth_txs ADD COLUMN priority_fee_per_gas BIGINT;
ALTER TABLE eth_txs ALTER COLUMN gas_price DROP NOT NULL;
ALTER TABLE eth_txs_history ADD COLUMN base_fee_per_gas BIGINT;
ALTER TABLE eth_txs_history ADD COLUMN priority_fee_per_gas BIGINT;
ALTER TABLE eth_txs_history ALTER COLUMN gas_price DROP NOT NULL;
