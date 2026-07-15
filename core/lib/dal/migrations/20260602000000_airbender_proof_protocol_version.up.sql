-- Persist the protocol semantic version a batch's Airbender FRI proof was generated under, so the
-- SNARK wrapping step reuses the exact same version (and blob key) instead of recomputing it.
ALTER TABLE airbender_proof_generation_details
    ADD COLUMN IF NOT EXISTS protocol_version INT,
    ADD COLUMN IF NOT EXISTS protocol_version_patch INT NOT NULL DEFAULT 0;
