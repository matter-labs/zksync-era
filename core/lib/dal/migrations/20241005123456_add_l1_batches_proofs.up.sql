CREATE TABLE l1_batches_proofs (
  l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
  proof JSONB NOT NULL
);
