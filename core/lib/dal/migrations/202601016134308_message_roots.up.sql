CREATE TABLE IF NOT EXISTS interop_roots (
    chain_id BIGINT NOT NULL,
    dependency_block_number BIGINT NOT NULL,
    processed_block_number BIGINT,
    interop_root_sides BYTEA[] NOT NULL,
    PRIMARY KEY (chain_id, dependency_block_number)
);

CREATE INDEX IF NOT EXISTS interop_roots_processed_block_number_idx ON interop_roots (processed_block_number);

ALTER TABLE l1_batches
    ADD COLUMN batch_chain_merkle_path_until_msg_root BYTEA;

ALTER TYPE event_type ADD VALUE 'InteropRoot';
