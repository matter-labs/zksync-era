UPDATE protocol_patches SET recursion_scheduler_level_vk_hash = snark_wrapper_vk_hash WHERE recursion_scheduler_level_vk_hash = ''::bytea;
ALTER TABLE protocol_patches DROP COLUMN snark_wrapper_vk_hash;
ALTER TABLE protocol_patches ALTER COLUMN recursion_scheduler_level_vk_hash DROP DEFAULT;
