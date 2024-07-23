ALTER TABLE protocol_versions DROP COLUMN IF EXISTS recursion_node_level_vk_hash;
ALTER TABLE protocol_versions DROP COLUMN IF EXISTS recursion_leaf_level_vk_hash;
ALTER TABLE protocol_versions DROP COLUMN IF EXISTS recursion_circuits_set_vks_hash;

ALTER TABLE protocol_patches DROP COLUMN IF EXISTS recursion_node_level_vk_hash;
ALTER TABLE protocol_patches DROP COLUMN IF EXISTS recursion_leaf_level_vk_hash;
ALTER TABLE protocol_patches DROP COLUMN IF EXISTS recursion_circuits_set_vks_hash;
