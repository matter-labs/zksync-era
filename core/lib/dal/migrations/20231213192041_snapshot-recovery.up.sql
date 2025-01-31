CREATE TABLE snapshot_recovery
(
    l1_batch_number        BIGINT    NOT NULL PRIMARY KEY,
    l1_batch_root_hash     BYTEA     NOT NULL,
    miniblock_number       BIGINT    NOT NULL,
    miniblock_root_hash    BYTEA     NOT NULL,

    last_finished_chunk_id INT,
    total_chunk_count     INT       NOT NULL,

    created_at             TIMESTAMP NOT NULL,
    updated_at             TIMESTAMP NOT NULL
)
