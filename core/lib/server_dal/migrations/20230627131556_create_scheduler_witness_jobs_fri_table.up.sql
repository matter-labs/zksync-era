CREATE TABLE IF NOT EXISTS scheduler_witness_jobs_fri
(
    l1_batch_number BIGINT PRIMARY KEY,
    scheduler_partial_input_blob_url TEXT NOT NULL,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
