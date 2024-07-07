-- Add down migration script here
ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS l2_to_l1_logs;
