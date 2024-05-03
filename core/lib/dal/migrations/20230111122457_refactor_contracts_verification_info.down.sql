ALTER TABLE contracts ADD COLUMN bytecode BYTEA;
ALTER TABLE contracts ADD COLUMN tx_hash BYTEA;
ALTER TABLE contracts ADD COLUMN miniblock_number BIGINT;

ALTER TABLE contracts_verification_info RENAME TO contracts;
