ALTER TABLE airbender_proof_generation_details DROP COLUMN IF EXISTS proof;
ALTER TABLE airbender_proof_generation_details ADD COLUMN proof_blob_url TEXT;
