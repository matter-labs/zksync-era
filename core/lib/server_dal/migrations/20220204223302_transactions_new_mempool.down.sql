ALTER TABLE transactions DELETE COLUMN mempool_preceding_nonce_gaps;
ALTER TABLE transactions ALTER COLUMN nonce SET NOT NULL;

DROP INDEX transactions_initiator_address_nonce;
