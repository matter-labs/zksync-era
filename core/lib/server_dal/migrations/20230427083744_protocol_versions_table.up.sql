CREATE TABLE IF NOT EXISTS protocol_versions (
    id INT PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    recursion_scheduler_level_vk_hash BYTEA NOT NULL,
    recursion_node_level_vk_hash BYTEA NOT NULL,
    recursion_leaf_level_vk_hash BYTEA NOT NULL,
    recursion_circuits_set_vks_hash BYTEA NOT NULL,
    bootloader_code_hash BYTEA NOT NULL,
    default_account_code_hash BYTEA NOT NULL,
    verifier_address BYTEA NOT NULL,
    upgrade_tx_hash BYTEA REFERENCES transactions (hash),
    created_at TIMESTAMP NOT NULL
);

ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES protocol_versions (id);
ALTER TABLE miniblocks ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES protocol_versions (id);
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS upgrade_id INT;
