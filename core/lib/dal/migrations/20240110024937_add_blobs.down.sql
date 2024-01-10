ALTER TABLE l1_batches DROP COLUMN blobs;
ALTER TABLE l1_batches DROP COLUMN versioned_hashes;

ALTER TABLE eth_txs DROP COLUMN blobs;
ALTER TABLE eth_txs DROP COLUMN versioned_hashes;
ALTER TABLE eth_txs DROP COLUMN blob_base_gas_fee;
