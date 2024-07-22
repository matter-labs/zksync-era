ALTER TABLE witness_inputs_fri ADD COLUMN IF NOT EXISTS merkle_tree_paths_blob_url TEXT;
ALTER TABLE witness_inputs_fri ADD COLUMN IF NOT EXISTS eip_4844_blobs TEXT;
ALTER TABLE witness_inputs_fri ADD COLUMN IF NOT EXISTS is_blob_cleaned BOOLEAN;
