-- Add down migration script here
ALTER TABLE l1_batches DROP COLUMN IF EXISTS sealed_at;
