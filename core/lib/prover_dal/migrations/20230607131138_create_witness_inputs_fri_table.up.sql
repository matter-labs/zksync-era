CREATE TABLE IF NOT EXISTS witness_inputs_fri
(
    l1_batch_number BIGINT NOT NULL PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
    merkle_tree_paths_blob_url TEXT,
    attempts SMALLINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    is_blob_cleaned BOOLEAN
    );
