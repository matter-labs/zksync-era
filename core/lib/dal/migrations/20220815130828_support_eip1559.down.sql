ALTER TABLE transactions ADD COLUMN IF NOT EXISTS ergs_price_limit NUMERIC(80);

ALTER TABLE transactions DROP COLUMN IF EXISTS max_fee_per_erg;
ALTER TABLE transactions DROP COLUMN IF EXISTS max_priority_fee_per_erg;
ALTER TABLE transactions DROP COLUMN IF EXISTS effective_gas_price NUMERIC(80);

ALTER TABLE blocks DROP COLUMN IF EXISTS base_fee_per_erg;
