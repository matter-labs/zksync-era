ALTER TABLE witness_inputs_fri ADD COLUMN IF NOT EXISTS merkle_tree_paths_blob_url TEXT;
ALTER TABLE witness_inputs_fri ADD COLUMN IF NOT EXISTS eip_4844_blobs TEXT;
ALTER TABLE witness_inputs_fri ADD COLUMN IF NOT EXISTS is_blob_cleaned BOOLEAN;
ALTER TABLE leaf_aggregation_witness_jobs_fri ADD COLUMN IF NOT EXISTS is_blob_cleaned BOOLEAN;
ALTER TABLE prover_jobs_fri ADD COLUMN IF NOT EXISTS is_blob_cleaned BOOLEAN;
ALTER TABLE prover_jobs_fri_archive ADD COLUMN IF NOT EXISTS is_blob_cleaned BOOLEAN;
