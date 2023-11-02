-- Add down migration script here

ALTER TABLE transactions DROP COLUMN l1_tx_mint;
ALTER TABLE transactions DROP COLUMN l1_tx_refund_recipient;
