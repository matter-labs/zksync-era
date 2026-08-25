-- `lock_batch_for_proving` reads the highest `(protocol_version, protocol_version_patch)` ever
-- assigned on every poll, as the floor new assignments must clear. Without this index that is a
-- sequential scan of one row per L1 batch.
CREATE INDEX IF NOT EXISTS idx_airbender_proof_generation_details_version_floor
    ON airbender_proof_generation_details (protocol_version DESC, protocol_version_patch DESC)
    WHERE protocol_version IS NOT NULL;
