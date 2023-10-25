CREATE TABLE IF NOT EXISTS proof_compression_jobs_fri
(
    l1_batch_number       BIGINT    NOT NULL PRIMARY KEY,
    attempts              SMALLINT  NOT NULL DEFAULT 0,
    status                TEXT      NOT NULL,
    fri_proof_blob_url    TEXT      NOT NULL,
    l1_proof_blob_url     TEXT,
    error                 TEXT,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken            TIME
);

CREATE INDEX IF NOT EXISTS idx_proof_compression_jobs_fri_status_processing_attempts
    ON proof_compression_jobs_fri (processing_started_at, attempts)
    WHERE status IN ('in_progress', 'failed');
