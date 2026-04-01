-- Restore TEE-specific columns and attestations table

CREATE TABLE IF NOT EXISTS airbender_attestations
(
    pubkey      BYTEA PRIMARY KEY,
    attestation BYTEA
);

ALTER TABLE airbender_proof_generation_details ADD COLUMN IF NOT EXISTS tee_type TEXT;
UPDATE airbender_proof_generation_details SET tee_type = 'sgx' WHERE tee_type IS NULL;
ALTER TABLE airbender_proof_generation_details ALTER COLUMN tee_type SET NOT NULL;

ALTER TABLE airbender_proof_generation_details DROP CONSTRAINT airbender_proof_generation_details_pkey;
ALTER TABLE airbender_proof_generation_details ADD PRIMARY KEY (l1_batch_number, tee_type);

ALTER TABLE airbender_proof_generation_details ADD COLUMN IF NOT EXISTS signature BYTEA;
ALTER TABLE airbender_proof_generation_details ADD COLUMN IF NOT EXISTS pubkey BYTEA REFERENCES airbender_attestations (pubkey) ON DELETE SET NULL;
