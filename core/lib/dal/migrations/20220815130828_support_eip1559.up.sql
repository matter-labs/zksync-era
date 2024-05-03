ALTER TABLE transactions DROP COLUMN IF EXISTS ergs_price_limit;

ALTER TABLE transactions ADD COLUMN IF NOT EXISTS max_fee_per_erg NUMERIC(80);
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS max_priority_fee_per_erg NUMERIC(80);
ALTER TABLE transactions ADD COLUMN IF NOT EXISTS effective_gas_price NUMERIC(80);

ALTER TABLE blocks ADD COLUMN IF NOT EXISTS base_fee_per_erg NUMERIC(80) NOT NULL DEFAULT 1;
