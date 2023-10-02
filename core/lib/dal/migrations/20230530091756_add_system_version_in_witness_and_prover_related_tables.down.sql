ALTER TABLE witness_inputs DROP COLUMN IF EXISTS protocol_version;
ALTER TABLE leaf_aggregation_witness_jobs DROP COLUMN IF EXISTS protocol_version;
ALTER TABLE node_aggregation_witness_jobs DROP COLUMN IF EXISTS protocol_version;
ALTER TABLE scheduler_witness_jobs DROP COLUMN IF EXISTS protocol_version;
ALTER TABLE prover_jobs DROP COLUMN IF EXISTS protocol_version;

