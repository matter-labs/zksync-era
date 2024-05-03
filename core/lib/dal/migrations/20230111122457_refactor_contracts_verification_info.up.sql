ALTER TABLE contracts DROP COLUMN bytecode;
ALTER TABLE contracts DROP COLUMN tx_hash;
ALTER TABLE contracts DROP COLUMN miniblock_number;

DELETE FROM contracts WHERE verification_info IS NULL;

ALTER TABLE contracts RENAME TO contracts_verification_info;
