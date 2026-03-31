ALTER TABLE airbender_proof_generation_details RENAME TO tee_proof_generation_details;
CREATE TABLE IF NOT EXISTS tee_attestations (
    pubkey BYTEA PRIMARY KEY,
    attestation BYTEA
);
ALTER TABLE tee_proof_generation_details
    ADD CONSTRAINT tee_proof_generation_details_pubkey_fkey
    FOREIGN KEY (pubkey) REFERENCES tee_attestations(pubkey) ON DELETE SET NULL;
