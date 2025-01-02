CREATE TABLE IF NOT EXISTS message_roots (
    chain_id BIGINT NOT NULL,
    block_number BIGINT NOT NULL,
    message_root_hash BYTEA NOT NULL
);