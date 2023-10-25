CREATE TABLE IF NOT EXISTS prover_jobs_fri
(
    id BIGSERIAL PRIMARY KEY,
    l1_batch_number BIGINT NOT NULL REFERENCES l1_batches (number) ON DELETE CASCADE,
    circuit_id SMALLINT NOT NULL,
    circuit_blob_url TEXT NOT NULL,
    aggregation_round SMALLINT NOT NULL,
    sequence_number INT NOT NULL,
    proof BYTEA,
    status TEXT NOT NULL,
    error TEXT,
    attempts SMALLINT NOT NULL DEFAULT 0,
    processing_started_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    time_taken TIME,
    is_blob_cleaned BOOLEAN
    );
