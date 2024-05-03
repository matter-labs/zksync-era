ALTER TABLE snapshot_recovery
    DROP COLUMN IF EXISTS last_finished_chunk_id,
    DROP COLUMN IF EXISTS total_chunk_count;
