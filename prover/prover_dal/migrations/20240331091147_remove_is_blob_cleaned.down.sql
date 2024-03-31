ALTER TABLE IF EXISTS witness_inputs_fri ADD COLUMN is_blob_cleaned BOOLEAN;
ALTER TABLE IF EXISTS leaf_aggregation_witness_jobs_fri ADD COLUMN is_blob_cleaned BOOLEAN;

ALTER TABLE IF EXISTS prover_jobs_fri ADD COLUMN is_blob_cleaned BOOLEAN;
ALTER TABLE IF EXISTS prover_jobs_fri_archive ADD COLUMN is_blob_cleaned BOOLEAN;
