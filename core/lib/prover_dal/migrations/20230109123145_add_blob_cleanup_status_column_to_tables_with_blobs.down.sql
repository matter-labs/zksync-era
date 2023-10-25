ALTER TABLE prover_jobs
    DROP COLUMN IF EXISTS is_blob_cleaned;

ALTER TABLE witness_inputs
    DROP COLUMN IF EXISTS is_blob_cleaned;

ALTER TABLE leaf_aggregation_witness_jobs
    DROP COLUMN IF EXISTS is_blob_cleaned;

ALTER TABLE node_aggregation_witness_jobs
    DROP COLUMN IF EXISTS is_blob_cleaned;

ALTER TABLE scheduler_witness_jobs
    DROP COLUMN IF EXISTS is_blob_cleaned;
