-- Remove legacy TEE-specific columns and the attestations table from airbender

-- Drop the foreign key and TEE-specific columns
ALTER TABLE airbender_proof_generation_details DROP COLUMN IF EXISTS pubkey;
ALTER TABLE airbender_proof_generation_details DROP COLUMN IF EXISTS signature;

-- Recreate primary key as just l1_batch_number (drop tee_type from composite PK)
-- First, delete any duplicate l1_batch_number rows (keep the most recently updated one)
DELETE FROM airbender_proof_generation_details a
    USING airbender_proof_generation_details b
WHERE a.l1_batch_number = b.l1_batch_number
  AND a.tee_type > b.tee_type;

ALTER TABLE airbender_proof_generation_details DROP CONSTRAINT airbender_proof_generation_details_pkey;
ALTER TABLE airbender_proof_generation_details ADD PRIMARY KEY (l1_batch_number);

ALTER TABLE airbender_proof_generation_details DROP COLUMN IF EXISTS tee_type;

-- Drop the attestations table
DROP TABLE IF EXISTS airbender_attestations;
