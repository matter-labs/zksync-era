ALTER TABLE prover_fri_protocol_versions ALTER COLUMN recursion_node_level_vk_hash SET NOT NULL;
ALTER TABLE prover_fri_protocol_versions ALTER COLUMN recursion_leaf_level_vk_hash SET NOT NULL;
ALTER TABLE prover_fri_protocol_versions ALTER COLUMN recursion_circuits_set_vks_hash SET NOT NULL;
