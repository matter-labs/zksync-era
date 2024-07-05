CREATE TABLE protocol_patches (
    minor INTEGER NOT NULL REFERENCES protocol_versions(id),
    patch INTEGER NOT NULL,
    recursion_scheduler_level_vk_hash BYTEA NOT NULL,
    recursion_node_level_vk_hash      BYTEA NOT NULL,
    recursion_leaf_level_vk_hash      BYTEA NOT NULL,
    recursion_circuits_set_vks_hash   BYTEA NOT NULL,
    created_at TIMESTAMP NOT NULL,
    PRIMARY KEY (minor, patch)
);

INSERT INTO protocol_patches
SELECT id as "minor", 0 as "patch", recursion_scheduler_level_vk_hash, recursion_node_level_vk_hash,
    recursion_leaf_level_vk_hash, recursion_circuits_set_vks_hash, now() as "created_at"
FROM protocol_versions;

ALTER TABLE protocol_versions ALTER COLUMN recursion_scheduler_level_vk_hash DROP NOT NULL;
ALTER TABLE protocol_versions ALTER COLUMN recursion_node_level_vk_hash DROP NOT NULL;
ALTER TABLE protocol_versions ALTER COLUMN recursion_leaf_level_vk_hash DROP NOT NULL;
ALTER TABLE protocol_versions ALTER COLUMN recursion_circuits_set_vks_hash DROP NOT NULL;
