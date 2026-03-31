ALTER TABLE tee_proof_generation_details DROP CONSTRAINT IF EXISTS tee_proof_generation_details_pubkey_fkey;
ALTER TABLE tee_proof_generation_details DROP CONSTRAINT IF EXISTS tee_proof_generation_details_pkey;
ALTER TABLE tee_proof_generation_details DROP COLUMN IF EXISTS tee_type;
ALTER TABLE tee_proof_generation_details DROP COLUMN IF EXISTS pubkey;
ALTER TABLE tee_proof_generation_details DROP COLUMN IF EXISTS signature;
ALTER TABLE tee_proof_generation_details ADD PRIMARY KEY (l1_batch_number);
ALTER TABLE tee_proof_generation_details RENAME TO airbender_proof_generation_details;
DROP TABLE IF EXISTS tee_attestations;
