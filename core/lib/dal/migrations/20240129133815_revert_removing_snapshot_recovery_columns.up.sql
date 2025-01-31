-- Temporary revert of the previous `snapshot_recovery` so that the migration is backward-compatible.
-- Do not use these columns in code.
ALTER TABLE snapshot_recovery
    ADD COLUMN IF NOT EXISTS last_finished_chunk_id INT,
    ADD COLUMN IF NOT EXISTS total_chunk_count INT NOT NULL DEFAULT 0;
