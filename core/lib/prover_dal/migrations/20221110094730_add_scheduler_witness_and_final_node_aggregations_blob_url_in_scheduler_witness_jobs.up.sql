ALTER TABLE scheduler_witness_jobs ADD COLUMN IF NOT EXISTS scheduler_witness_blob_url TEXT;
ALTER TABLE scheduler_witness_jobs ADD COLUMN IF NOT EXISTS final_node_aggregations_blob_url TEXT;
