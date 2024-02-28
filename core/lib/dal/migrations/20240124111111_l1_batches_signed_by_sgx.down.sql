ALTER TABLE l1_batches DROP COLUMN IF EXISTS signed_state_root;
ALTER TABLE l1_batches DROP COLUMN IF EXISTS state_root_signing_pubkey;

DROP TABLE IF EXISTS sgx_attestation;
