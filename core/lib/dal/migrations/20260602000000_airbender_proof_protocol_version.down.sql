ALTER TABLE airbender_proof_generation_details
    DROP COLUMN IF EXISTS protocol_version,
    DROP COLUMN IF EXISTS protocol_version_patch;
