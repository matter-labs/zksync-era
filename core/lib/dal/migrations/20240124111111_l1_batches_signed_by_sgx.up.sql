ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS signed_state_root BYTEA;
ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS state_root_signing_pubkey BYTEA;

CREATE TABLE IF NOT EXISTS sgx_attestation (
    public_key BYTEA NOT NULL PRIMARY KEY,
    attestation BYTEA NOT NULL
);
