-- Add up migration script here

ALTER TABLE factory_deps DROP CONSTRAINT factory_deps_miniblock_number_fkey;
ALTER TABLE initial_writes DROP CONSTRAINT initial_writes_l1_batch_number_fkey;
ALTER TABLE storage_logs DROP CONSTRAINT storage_logs_miniblock_number_fkey;

CREATE TABLE applied_snapshot_status
(
    l1_batch_number         BIGINT NOT NULL PRIMARY KEY,
    is_finished             BOOL NOT NULL,
    last_finished_chunk_id   INT
)
