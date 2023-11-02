-- Add up migration script here

ALTER TABLE transactions ADD COLUMN l1_tx_mint NUMERIC;
ALTER TABLE transactions ADD COLUMN l1_tx_refund_recipient BYTEA;
