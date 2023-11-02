ALTER TABLE leaf_aggregation_witness_jobs ADD COLUMN IF NOT EXISTS basic_circuits_blob_url TEXT;
ALTER TABLE leaf_aggregation_witness_jobs ADD COLUMN IF NOT EXISTS basic_circuits_inputs_blob_url TEXT;
