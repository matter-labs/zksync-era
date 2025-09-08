ALTER TABLE l1_batches ADD COLUMN settlement_layer_type TEXT DEFAULT 'L1';
ALTER TABLE l1_batches ADD COLUMN settlement_layer_chain_id BIGINT;
