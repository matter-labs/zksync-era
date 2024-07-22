ALTER TABLE witness_inputs_fri DROP COLUMN IF EXISTS merkle_tree_paths_blob_url;
ALTER TABLE witness_inputs_fri DROP COLUMN IF EXISTS eip_4844_blobs;
ALTER TABLE witness_inputs_fri DROP COLUMN IF EXISTS is_blob_cleaned;
