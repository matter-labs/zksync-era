ALTER TABLE airbender_proof_generation_details RENAME TO tee_proof_generation_details;
ALTER TABLE tee_proof_generation_details DROP CONSTRAINT IF EXISTS tee_proof_generation_details_pkey;
ALTER TABLE tee_proof_generation_details ADD COLUMN tee_type TEXT NOT NULL DEFAULT 'sgx';
ALTER TABLE tee_proof_generation_details ADD COLUMN pubkey BYTEA;
ALTER TABLE tee_proof_generation_details ADD COLUMN signature BYTEA;
ALTER TABLE tee_proof_generation_details ADD PRIMARY KEY (l1_batch_number, tee_type);
CREATE TABLE IF NOT EXISTS tee_attestations (
    pubkey BYTEA PRIMARY KEY,
    attestation BYTEA
);
ALTER TABLE tee_proof_generation_details
    ADD CONSTRAINT tee_proof_generation_details_pubkey_fkey
    FOREIGN KEY (pubkey) REFERENCES tee_attestations(pubkey) ON DELETE SET NULL;
