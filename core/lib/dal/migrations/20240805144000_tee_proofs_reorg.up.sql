ALTER TABLE tee_proof_generation_details DROP CONSTRAINT tee_proof_generation_details_pkey;
UPDATE tee_proof_generation_details SET tee_type = 'sgx' WHERE tee_type IS NULL;
ALTER TABLE tee_proof_generation_details ALTER COLUMN tee_type SET NOT NULL;
ALTER TABLE tee_proof_generation_details ALTER COLUMN l1_batch_number SET NOT NULL;
ALTER TABLE tee_proof_generation_details ADD PRIMARY KEY (l1_batch_number, tee_type);
