ALTER TABLE transactions ADD COLUMN mempool_preceding_nonce_gaps SMALLINT;
ALTER TABLE transactions ALTER COLUMN nonce DROP NOT NULL;

UPDATE transactions SET nonce = NULL where transactions.is_priority = TRUE;
UPDATE transactions SET mempool_preceding_nonce_gaps = 0 where transactions.is_priority = FALSE;

DELETE FROM transactions WHERE block_number IS NULL and error IS NOT NULL;

CREATE UNIQUE INDEX transactions_initiator_address_nonce ON transactions (initiator_address, nonce);
