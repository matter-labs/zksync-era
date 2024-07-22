ALTER TABLE witness_inputs_fri DROP COLUMN IF EXISTS merkle_tree_paths_blob_url;
ALTER TABLE witness_inputs_fri DROP COLUMN IF EXISTS eip_4844_blobs;
ALTER TABLE witness_inputs_fri DROP COLUMN IF EXISTS is_blob_cleaned;
ALTER TABLE leaf_aggregation_witness_jobs_fri DROP COLUMN IF EXISTS is_blob_cleaned;
ALTER TABLE prover_jobs_fri DROP COLUMN IF EXISTS is_blob_cleaned;
ALTER TABLE prover_jobs_fri_archive DROP COLUMN IF EXISTS is_blob_cleaned;
