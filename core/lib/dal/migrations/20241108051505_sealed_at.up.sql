-- add a sealed_at column for metrics
ALTER TABLE l1_batches ADD COLUMN sealed_at TIMESTAMP;
