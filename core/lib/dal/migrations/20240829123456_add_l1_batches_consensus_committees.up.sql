ALTER TABLE consensus_replica_state
  ADD COLUMN global_config JSONB NULL;

CREATE TABLE l1_batches_consensus_committees (
  l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
  attesters JSONB NOT NULL,
  updated_at TIMESTAMP NOT NULL
); 
