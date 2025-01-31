CREATE TYPE basic_witness_input_producer_job_status AS ENUM ('Queued', 'ManuallySkipped', 'InProgress', 'Successful', 'Failed');

CREATE TABLE IF NOT EXISTS basic_witness_input_producer_jobs
(
    l1_batch_number       BIGINT    NOT NULL PRIMARY KEY,
    attempts              SMALLINT  NOT NULL DEFAULT 0,
    status                basic_witness_input_producer_job_status,
    picked_by             TEXT,
    input_blob_url        TEXT,
    error                 TEXT,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken            TIME
);

CREATE INDEX IF NOT EXISTS idx_basic_witness_input_producer_jobs_status_processing_attempts
    ON basic_witness_input_producer_jobs (status, processing_started_at, attempts);
