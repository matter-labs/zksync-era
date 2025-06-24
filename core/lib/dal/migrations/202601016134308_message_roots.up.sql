CREATE TABLE IF NOT EXISTS interop_roots (
    chain_id BIGINT NOT NULL,
    dependency_block_number BIGINT NOT NULL,
    processed_block_number BIGINT,
    interop_root_sides BYTEA[] NOT NULL,
    received_timestamp BIGINT NOT NULL,
    PRIMARY KEY (chain_id, dependency_block_number)
);

ALTER TABLE l1_batches
    ADD COLUMN batch_chain_merkle_path_until_msg_root BYTEA;

ALTER TABLE miniblocks
    ADD COLUMN interop_roots_assigned BOOLEAN NOT NULL DEFAULT FALSE;

-- postgres doesn't allow dropping enum variant, so nothing is done in down.sql
ALTER TYPE event_type ADD VALUE 'ChainBatchRootInterop';
ALTER TYPE event_type ADD VALUE 'InteropRoot';
