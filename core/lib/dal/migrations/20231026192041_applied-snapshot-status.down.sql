-- Add down migration script here

ALTER TABLE factory_deps ADD CONSTRAINT factory_deps_miniblock_number_fkey
    FOREIGN KEY (miniblock_number) REFERENCES miniblocks (number);
ALTER TABLE initial_writes ADD CONSTRAINT initial_writes_l1_batch_number_fkey
    FOREIGN KEY (l1_batch_number) REFERENCES l1_batches (number);
ALTER TABLE storage_logs ADD CONSTRAINT storage_logs_miniblock_number_fkey
    FOREIGN KEY (miniblock_number) REFERENCES miniblocks (number);

DROP TABLE IF EXISTS applied_snapshot_status;
