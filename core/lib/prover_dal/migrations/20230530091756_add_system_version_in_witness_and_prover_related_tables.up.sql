ALTER TABLE witness_inputs ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES protocol_versions (id);
ALTER TABLE leaf_aggregation_witness_jobs ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES protocol_versions (id);
ALTER TABLE node_aggregation_witness_jobs ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES protocol_versions (id);
ALTER TABLE scheduler_witness_jobs ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES protocol_versions (id);
ALTER TABLE prover_jobs ADD COLUMN IF NOT EXISTS protocol_version INT REFERENCES protocol_versions (id);
