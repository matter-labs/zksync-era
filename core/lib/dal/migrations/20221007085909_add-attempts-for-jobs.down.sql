ALTER TABLE prover_jobs DROP COLUMN attempts;
ALTER TABLE witness_inputs DROP COLUMN attempts;
ALTER TABLE leaf_aggregation_witness_jobs DROP COLUMN attempts;
ALTER TABLE node_aggregation_witness_jobs DROP COLUMN attempts;
ALTER TABLE scheduler_witness_jobs DROP COLUMN attempts;
ALTER TABLE contract_verification_requests DROP COLUMN attempts;
