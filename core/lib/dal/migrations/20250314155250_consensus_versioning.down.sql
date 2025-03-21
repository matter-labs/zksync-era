ALTER TABLE miniblocks_consensus ALTER COLUMN certificate SET NOT NULL;
ALTER TABLE miniblocks_consensus DROP COLUMN versioned_certificate;
ALTER TABLE consensus_replica_state ADD COLUMN genesis JSONB;
