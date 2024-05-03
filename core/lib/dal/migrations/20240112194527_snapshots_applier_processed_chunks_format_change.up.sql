ALTER TABLE snapshot_recovery DROP COLUMN last_finished_chunk_id;
ALTER TABLE snapshot_recovery DROP COLUMN total_chunk_count;

ALTER TABLE snapshot_recovery ADD COLUMN storage_logs_chunks_processed BOOL[] NOT NULL;

ALTER TABLE factory_deps DROP CONSTRAINT factory_deps_miniblock_number_fkey;
ALTER TABLE initial_writes DROP CONSTRAINT initial_writes_l1_batch_number_fkey;
ALTER TABLE storage_logs DROP CONSTRAINT storage_logs_miniblock_number_fkey;
