ALTER TABLE miniblocks ADD COLUMN settlement_layer_type TEXT DEFAULT 'L1';
ALTER TABLE miniblocks ADD COLUMN settlement_layer_chain_id BIGINT;
