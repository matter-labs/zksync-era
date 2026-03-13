-- Add up migration script here
ALTER TABLE l1_batches DROP COLUMN IF EXISTS l2_to_l1_logs;
