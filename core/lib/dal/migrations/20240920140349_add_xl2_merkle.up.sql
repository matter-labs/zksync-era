ALTER TABLE transactions ADD COLUMN IF NOT EXISTS merkle_proof BYTEA;
