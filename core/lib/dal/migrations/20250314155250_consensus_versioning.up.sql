ALTER TABLE miniblocks_consensus ADD COLUMN versioned_certificate JSONB NULL;
ALTER TABLE miniblocks_consensus ALTER COLUMN certificate DROP NOT NULL;
