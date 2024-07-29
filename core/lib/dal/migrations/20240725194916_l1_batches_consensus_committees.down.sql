ALTER TABLE l1_batches_consensus
DROP COLUMN validator_committee,
DROP COLUMN attester_committee;

ALTER TABLE l1_batches_consensus
ALTER COLUMN certificate SET NOT NULL; 
