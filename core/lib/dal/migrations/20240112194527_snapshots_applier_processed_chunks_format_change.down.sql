ALTER TABLE snapshot_recovery ADD COLUMN last_finished_chunk_id NOT NULL;
ALTER TABLE snapshot_recovery ADD COLUMN total_chunk_count NOT NULL;

ALTER TABLE snapshot_recovery DROP COLUMN storage_logs_chunks_processed;

ALTER TABLE factory_deps ADD CONSTRAINT factory_deps_miniblock_number_fkey
    FOREIGN KEY (miniblock_number) REFERENCES miniblocks (number);
ALTER TABLE initial_writes ADD CONSTRAINT initial_writes_l1_batch_number_fkey
    FOREIGN KEY (l1_batch_number) REFERENCES l1_batches (number);
ALTER TABLE storage_logs ADD CONSTRAINT storage_logs_miniblock_number_fkey
    FOREIGN KEY (miniblock_number) REFERENCES miniblocks (number);
