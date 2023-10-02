CREATE TABLE IF NOT EXISTS node_aggregation_witness_jobs_fri
(
    id BIGSERIAL PRIMARY KEY,
    l1_batch_number BIGINT NOT NULL,
    circuit_id SMALLINT NOT NULL,
    depth INT NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    attempts SMALLINT NOT NULL DEFAULT 0,
    aggregations_url TEXT,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
