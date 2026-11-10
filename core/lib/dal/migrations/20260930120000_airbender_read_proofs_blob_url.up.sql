-- Not on `airbender_proof_generation_details`: its rows only appear once a prover claims a batch.
ALTER TABLE proof_generation_details ADD COLUMN IF NOT EXISTS airbender_read_proofs_blob_url TEXT;
