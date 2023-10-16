CREATE TABLE IF NOT EXISTS basic_witness_input_producer_jobs
(
    l1_batch_number       BIGINT    NOT NULL PRIMARY KEY,
    attempts              SMALLINT  NOT NULL DEFAULT 0,
    status                TEXT      NOT NULL,
    picked_by             TEXT,
    input_blob_url        TEXT,
    error                 TEXT,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken            TIME,
    protocol_version      INT       REFERENCES protocol_versions (id)
    );

CREATE INDEX IF NOT EXISTS idx_basic_witness_input_producer_jobs_status
    ON basic_witness_input_producer_jobs (status);

CREATE INDEX IF NOT EXISTS idx_basic_witness_input_producer_jobs_attempts_processing_status
    ON basic_witness_input_producer_jobs (processing_started_at, attempts, status);

CREATE INDEX IF NOT EXISTS idx_basic_witness_input_producer_jobs_time
    ON basic_witness_input_producer_jobs (time_taken);