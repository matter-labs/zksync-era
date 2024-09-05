ALTER TABLE protocol_patches ADD COLUMN snark_wrapper_vk_hash BYTEA NOT NULL;
ALTER TABLE protocol_patches ALTER COLUMN recursion_scheduler_level_vk_hash SET DEFAULT ''::bytea;
UPDATE protocol_patches SET snark_wrapper_vk_hash = recursion_scheduler_level_vk_hash;

-- Old column should be removed once the migration is on the mainnet.
COMMENT ON COLUMN protocol_patches.recursion_scheduler_level_vk_hash IS 'This column is deprecated and will be removed in the future. Use snark_wrapper_vk_hash instead.';
