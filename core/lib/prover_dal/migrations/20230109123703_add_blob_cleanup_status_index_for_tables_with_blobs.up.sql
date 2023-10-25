CREATE INDEX IF NOT EXISTS prover_jobs_blob_cleanup_status_index ON prover_jobs (status, is_blob_cleaned);

CREATE INDEX IF NOT EXISTS witness_inputs_blob_cleanup_status_index ON witness_inputs (status, is_blob_cleaned);

CREATE INDEX IF NOT EXISTS leaf_aggregation_witness_jobs_blob_cleanup_status_index ON leaf_aggregation_witness_jobs (status, is_blob_cleaned);

CREATE INDEX IF NOT EXISTS node_aggregation_witness_jobs_blob_cleanup_status_index ON node_aggregation_witness_jobs (status, is_blob_cleaned);

CREATE INDEX IF NOT EXISTS scheduler_witness_jobs_blob_cleanup_status_index ON scheduler_witness_jobs (status, is_blob_cleaned);
