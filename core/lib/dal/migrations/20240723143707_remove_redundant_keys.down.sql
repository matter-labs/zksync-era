ALTER TABLE protocol_versions ADD COLUMN IF NOT EXISTS recursion_node_level_vk_hash;
ALTER TABLE protocol_versions ADD COLUMN IF NOT EXISTS recursion_leaf_level_vk_hash;
ALTER TABLE protocol_versions ADD COLUMN IF NOT EXISTS recursion_circuits_set_vks_hash;

ALTER TABLE protocol_patches ADD COLUMN IF NOT EXISTS recursion_node_level_vk_hash;
ALTER TABLE protocol_patches ADD COLUMN IF NOT EXISTS recursion_leaf_level_vk_hash;
ALTER TABLE protocol_patches ADD COLUMN IF NOT EXISTS recursion_circuits_set_vks_hash;
