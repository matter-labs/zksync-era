ALTER TABLE miniblocks_consensus ALTER COLUMN certificate SET NOT NULL;
ALTER TABLE miniblocks_consensus DROP COLUMN versioned_certificate;