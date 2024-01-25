CREATE TABLE IF NOT EXISTS witness_inputs
(
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    merkle_tree_paths BYTEA,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    status TEXT NOT NULL,
    time_taken TIME DEFAULT '00:00:00'::TIME WITHOUT TIME ZONE NOT NULL,
    processing_started_at TIMESTAMP,
    error VARCHAR,
    attempts INTEGER DEFAULT 0 NOT NULL,
    merkel_tree_paths_blob_url TEXT,
    is_blob_cleaned boolean DEFAULT false NOT NULL,
    protocol_version INTEGER
    CONSTRAINT witness_inputs_prover_protocol_version_fkey REFERENCES prover_protocol_versions
);
CREATE INDEX IF NOT EXISTS witness_inputs_blob_cleanup_status_index ON witness_inputs (status, is_blob_cleaned);


CREATE TABLE leaf_aggregation_witness_jobs
(
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    basic_circuits BYTEA NOT NULL,
    basic_circuits_inputs BYTEA NOT NULL,
    number_of_basic_circuits INTEGER NOT NULL,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    attempts INTEGER DEFAULT 0 NOT NULL,
    basic_circuits_blob_url TEXT,
    basic_circuits_inputs_blob_url TEXT,
    is_blob_cleaned BOOLEAN DEFAULT FALSE NOT NULL,
    protocol_version INTEGER
    CONSTRAINT leaf_aggregation_witness_jobs_prover_protocol_version_fkey REFERENCES prover_protocol_versions
);
CREATE INDEX IF NOT EXISTS leaf_aggregation_witness_jobs_blob_cleanup_status_index ON leaf_aggregation_witness_jobs (status, is_blob_cleaned);


CREATE TABLE node_aggregation_witness_jobs
(
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    leaf_layer_subqueues BYTEA,
    aggregation_outputs BYTEA,
    number_of_leaf_circuits INTEGER,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    attempts INTEGER DEFAULT 0 NOT NULL,
    leaf_layer_subqueues_blob_url TEXT,
    aggregation_outputs_blob_url TEXT,
    is_blob_cleaned BOOLEAN DEFAULT FALSE NOT NULL,
    protocol_version INTEGER
    CONSTRAINT node_aggregation_witness_jobs_prover_protocol_version_fkey REFERENCES prover_protocol_versions
);
CREATE INDEX IF NOT EXISTS node_aggregation_witness_jobs_blob_cleanup_status_index ON node_aggregation_witness_jobs (status, is_blob_cleaned);


CREATE TABLE scheduler_witness_jobs
(
    l1_batch_number BIGINT NOT NULL PRIMARY KEY,
    scheduler_witness BYTEA NOT NULL,
    final_node_aggregations BYTEA,
    status TEXT NOT NULL,
    processing_started_at TIMESTAMP,
    time_taken TIME,
    error TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    attempts INTEGER DEFAULT 0 NOT NULL,
    aggregation_result_coords BYTEA,
    scheduler_witness_blob_url TEXT,
    final_node_aggregations_blob_url TEXT,
    is_blob_cleaned BOOLEAN DEFAULT FALSE NOT NULL,
    protocol_version INTEGER
    CONSTRAINT scheduler_witness_jobs_prover_protocol_version_fkey REFERENCES prover_protocol_versions
);
CREATE INDEX IF NOT EXISTS scheduler_witness_jobs_blob_cleanup_status_index ON scheduler_witness_jobs (status, is_blob_cleaned);
