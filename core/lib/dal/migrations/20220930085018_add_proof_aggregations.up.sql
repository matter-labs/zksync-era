CREATE TABLE IF NOT EXISTS leaf_aggregation_witness_jobs
(
    l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,

    basic_circuits BYTEA NOT NULL,
    basic_circuits_inputs BYTEA NOT NULL,

    number_of_basic_circuits INT NOT NULL,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE TABLE IF NOT EXISTS node_aggregation_witness_jobs
(
    l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,

    leaf_layer_subqueues BYTEA NOT NULL,
    aggregation_outputs BYTEA NOT NULL,

    number_of_leaf_circuits INT NOT NULL,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

-- 0 for basic, 1 for leaf, 2 for node, 3 for scheduler
ALTER TABLE prover_jobs ADD COLUMN aggregation_round INT NOT NULL DEFAULT 0;

