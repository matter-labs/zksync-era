-- Add up migration script here
CREATE TABLE IF NOT EXISTS leaf_aggregation_witness_jobs_fri
(
    id BIGSERIAL PRIMARY KEY,
    l1_batch_number BIGINT NOT NULL,
    circuit_id SMALLINT NOT NULL,
    closed_form_inputs_blob_url TEXT,
    attempts SMALLINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    is_blob_cleaned BOOLEAN
);
