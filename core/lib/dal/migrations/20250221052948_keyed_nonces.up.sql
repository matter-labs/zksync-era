DROP INDEX transactions_initiator_address_nonce;

ALTER TABLE transactions ADD COLUMN nonce_key numeric(80) NOT NULL DEFAULT 0;

CREATE UNIQUE INDEX transactions_initiator_address_nonce ON transactions (initiator_address, nonce_key, nonce);

