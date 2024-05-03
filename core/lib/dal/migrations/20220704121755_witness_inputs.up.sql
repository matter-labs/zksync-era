CREATE TABLE IF NOT EXISTS witness_inputs
(
    block_number BIGINT NOT NULL REFERENCES blocks (number) ON DELETE CASCADE,
    merkle_tree_paths BYTEA,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE INDEX IF NOT EXISTS witness_inputs_block_number_idx ON witness_inputs (block_number);
