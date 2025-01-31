DROP TABLE miniblocks_consensus;
ALTER TABLE miniblocks ADD COLUMN consensus JSONB NULL;
