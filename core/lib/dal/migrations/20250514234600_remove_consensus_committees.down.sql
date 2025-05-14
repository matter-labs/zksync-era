CREATE TABLE consensus_committees (
  active_at_block BIGINT PRIMARY KEY,
  validators JSONB NOT NULL
);
CREATE TABLE l1_batches_consensus (
  l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
  certificate JSONB NOT NULL,
  created_at TIMESTAMP NOT NULL,
  updated_at TIMESTAMP NOT NULL,
  CHECK((certificate->'message'->'number')::jsonb::numeric = l1_batch_number)
);
CREATE TABLE l1_batches_consensus_committees (
  l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
  attesters JSONB NOT NULL,
  updated_at TIMESTAMP NOT NULL
);
ALTER TABLE miniblocks_consensus ADD COLUMN certificate JSONB NOT NULL; 