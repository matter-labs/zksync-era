ALTER TABLE tee_verifier_input_producer_jobs ADD COLUMN picked_by TEXT;

ALTER TABLE tee_proof_generation_details DROP CONSTRAINT tee_proof_generation_details_pkey;
ALTER TABLE tee_proof_generation_details ALTER COLUMN tee_type DROP NOT NULL;
ALTER TABLE tee_proof_generation_details ADD PRIMARY KEY (l1_batch_number);
