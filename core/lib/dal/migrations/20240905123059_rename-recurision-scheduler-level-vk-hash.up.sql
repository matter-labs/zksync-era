ALTER TABLE protocol_patches ADD COLUMN snark_wrapper_vk_hash BYTEA NOT NULL DEFAULT ''::bytea;
ALTER TABLE protocol_patches ALTER COLUMN recursion_scheduler_level_vk_hash SET DEFAULT ''::bytea;
UPDATE protocol_patches SET snark_wrapper_vk_hash = recursion_scheduler_level_vk_hash;
-- Default was only needed to migrate old rows, we don't want this field to be forgotten by accident after migration.
ALTER TABLE protocol_patches ALTER COLUMN snark_wrapper_vk_hash DROP DEFAULT;

-- Old column should be removed once the migration is on the mainnet.
COMMENT ON COLUMN protocol_patches.recursion_scheduler_level_vk_hash IS 'This column is deprecated and will be removed in the future. Use snark_wrapper_vk_hash instead.';
