DROP INDEX transactions_initiator_address_nonce;

ALTER TABLE transactions ALTER COLUMN nonce SET DATA TYPE numeric(80);

CREATE UNIQUE INDEX transactions_initiator_address_nonce ON transactions (initiator_address, nonce);

