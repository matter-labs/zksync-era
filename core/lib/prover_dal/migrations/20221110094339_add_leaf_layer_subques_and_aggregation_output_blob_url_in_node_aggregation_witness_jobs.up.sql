ALTER TABLE node_aggregation_witness_jobs ADD COLUMN IF NOT EXISTS leaf_layer_subqueues_blob_url TEXT;
ALTER TABLE node_aggregation_witness_jobs ADD COLUMN IF NOT EXISTS aggregation_outputs_blob_url TEXT;
