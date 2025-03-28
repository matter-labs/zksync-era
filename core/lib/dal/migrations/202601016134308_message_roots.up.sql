CREATE TABLE IF NOT EXISTS message_roots (
    chain_id BIGINT NOT NULL,
    block_number BIGINT NOT NULL,
    message_root_sides BYTEA[] NOT NULL,
    PRIMARY KEY (chain_id, block_number)
);