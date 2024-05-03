ALTER TABLE eth_txs DROP COLUMN base_fee_per_gas;
ALTER TABLE eth_txs DROP COLUMN priority_fee_per_gas;
ALTER TABLE eth_txs ALTER COLUMN gas_price SET NOT NULL;
ALTER TABLE eth_txs_history DROP COLUMN base_fee_per_gas;
ALTER TABLE eth_txs_history DROP COLUMN priority_fee_per_gas;
ALTER TABLE eth_txs_history ALTER COLUMN gas_price SET NOT NULL;
