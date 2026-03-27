ALTER TABLE l1_batches ADD COLUMN settlement_layer_type TEXT NOT NULL;
ALTER TABLE l1_batches ADD COLUMN settlement_layer_chain_id BIGINT NOT NULL;
ALTER TABLE l1_batches ADD CONSTRAINT l1_batches_settlement_layer_type_check CHECK (settlement_layer_type IN ('L1', 'Gateway'));
