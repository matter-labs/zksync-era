CREATE TABLE IF NOT EXISTS interop_roots (
    chain_id BIGINT NOT NULL,
    dependency_block_number BIGINT NOT NULL,
    processed_block_number BIGINT,
    interop_root_sides BYTEA[] NOT NULL,
    received_timestamp BIGINT NOT NULL,
    PRIMARY KEY (chain_id, dependency_block_number)
);