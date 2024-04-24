ALTER TABLE IF EXISTS witness_inputs_fri DROP COLUMN IF EXISTS is_blob_cleaned;
ALTER TABLE IF EXISTS leaf_aggregation_witness_jobs_fri DROP COLUMN IF EXISTS is_blob_cleaned;

ALTER TABLE IF EXISTS prover_jobs_fri DROP COLUMN IF EXISTS is_blob_cleaned;
ALTER TABLE IF EXISTS prover_jobs_fri_archive DROP COLUMN IF EXISTS is_blob_cleaned;
