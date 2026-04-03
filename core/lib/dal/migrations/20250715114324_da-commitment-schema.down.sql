ALTER TABLE miniblocks ALTER COLUMN l2_da_validator_address SET NOT NULL;
ALTER TABLE miniblocks DROP COLUMN l2_da_commitment_scheme;
