ALTER TABLE transactions ADD COLUMN IF NOT EXISTS contract_address BYTEA;
CREATE INDEX IF NOT EXISTS transactions_contract_address_idx ON transactions (contract_address);
