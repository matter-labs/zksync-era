ALTER TABLE consensus_replica_state DROP COLUMN genesis;
ALTER TABLE miniblocks_consensus ADD COLUMN versioned_certificate JSONB NOT NULL;
ALTER TABLE miniblocks_consensus ALTER COLUMN certificate DROP NOT NULL;
