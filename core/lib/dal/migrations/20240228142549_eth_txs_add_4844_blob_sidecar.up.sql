ALTER TABLE eth_txs_history ADD COLUMN blob_base_fee_per_gas BIGINT;
ALTER TABLE eth_txs ADD COLUMN blob_sidecar BYTEA;
