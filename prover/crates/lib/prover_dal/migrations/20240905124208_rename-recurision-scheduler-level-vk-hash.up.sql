ALTER TABLE prover_fri_protocol_versions ADD COLUMN snark_wrapper_vk_hash BYTEA NOT NULL DEFAULT ''::bytea;
ALTER TABLE prover_fri_protocol_versions ALTER COLUMN recursion_scheduler_level_vk_hash SET DEFAULT ''::bytea;
UPDATE prover_fri_protocol_versions SET snark_wrapper_vk_hash = recursion_scheduler_level_vk_hash;
-- Default was only needed to migrate old rows, we don't want this field to be forgotten by accident after migration.
ALTER TABLE prover_fri_protocol_versions ALTER COLUMN snark_wrapper_vk_hash DROP DEFAULT;

-- Old column should be removed once the migration is on the mainnet.
COMMENT ON COLUMN prover_fri_protocol_versions.recursion_scheduler_level_vk_hash IS 'This column is deprecated and will be removed in the future. Use snark_wrapper_vk_hash instead.';
