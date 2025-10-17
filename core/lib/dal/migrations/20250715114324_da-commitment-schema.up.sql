ALTER TABLE miniblocks ADD COLUMN IF NOT EXISTS l2_da_commitment_scheme INTEGER;
ALTER TABLE miniblocks ALTER COLUMN l2_da_validator_address DROP NOT NULL;

