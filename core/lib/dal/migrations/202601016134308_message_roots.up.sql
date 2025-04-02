CREATE TABLE IF NOT EXISTS message_roots (
    chain_id BIGINT NOT NULL,
    dependency_block_number BIGINT NOT NULL,
    processed_block_number BIGINT,
    message_root_sides BYTEA[] NOT NULL,
    PRIMARY KEY (chain_id, dependency_block_number)
);