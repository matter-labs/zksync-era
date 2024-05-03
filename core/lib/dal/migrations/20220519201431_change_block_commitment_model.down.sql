-- Add down migration script here
ALTER TABLE blocks ADD COLUMN processable_onchain_ops BYTEA[] NOT NULL;
