ALTER TABLE l1_batches ADD COLUMN blobs BYTEA;
ALTER TABLE l1_batches ADD COLUMN versioned_hashes BYTEA;

ALTER TABLE eth_txs ADD COLUMN blobs BYTEA;
ALTER TABLE eth_txs ADD COLUMN versioned_hashes BYTEA;
ALTER TABLE eth_txs ADD COLUMN blob_base_gas_fee BIGINT;
