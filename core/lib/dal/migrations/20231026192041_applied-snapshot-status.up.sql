-- Add up migration script here

CREATE TABLE applied_snapshot_status
(
    l1_batch_number         BIGINT NOT NULL PRIMARY KEY,
    is_finished             BOOL NOT NULL,
    last_finished_chunk_id   INT
)
