CREATE TABLE l1_batches_consensus (
  l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
  certificate JSONB NOT NULL,
  
  created_at TIMESTAMP NOT NULL,
  updated_at TIMESTAMP NOT NULL,

  CHECK((certificate->'message'->'number')::jsonb::numeric = l1_batch_number)
);
