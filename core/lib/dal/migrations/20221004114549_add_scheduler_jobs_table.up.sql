CREATE TABLE IF NOT EXISTS scheduler_witness_jobs
(
    l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,

    scheduler_witness BYTEA NOT NULL,
    final_node_aggregations BYTEA,

    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

ALTER TABLE node_aggregation_witness_jobs ALTER COLUMN leaf_layer_subqueues DROP NOT NULL;
ALTER TABLE node_aggregation_witness_jobs ALTER COLUMN aggregation_outputs DROP NOT NULL;
ALTER TABLE node_aggregation_witness_jobs ALTER COLUMN number_of_leaf_circuits DROP NOT NULL;
