ALTER TABLE transactions DROP COLUMN IF EXISTS upgrade_id;
ALTER TABLE miniblocks DROP COLUMN IF EXISTS protocol_version;
ALTER TABLE l1_batches DROP COLUMN IF EXISTS protocol_version;
DROP TABLE IF EXISTS protocol_versions;
