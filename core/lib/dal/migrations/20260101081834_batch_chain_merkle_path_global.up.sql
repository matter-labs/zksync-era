ALTER TABLE l1_batches
    ADD COLUMN batch_chain_global_merkle_path BYTEA;

-- postgres doesn't allow dropping enum variant, so nothing is done in down.sql
ALTER TYPE event_type ADD VALUE 'ChainBatchRootGlobal';
