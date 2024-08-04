ALTER TABLE l1_batches_consensus
DROP COLUMN committee;

ALTER TABLE l1_batches_consensus
ALTER COLUMN certificate SET NOT NULL;
