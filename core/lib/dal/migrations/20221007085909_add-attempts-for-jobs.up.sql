ALTER TABLE prover_jobs ADD COLUMN attempts INT NOT NULL DEFAULT 0;
ALTER TABLE witness_inputs ADD COLUMN attempts INT NOT NULL DEFAULT 0;
ALTER TABLE leaf_aggregation_witness_jobs ADD COLUMN attempts INT NOT NULL DEFAULT 0;
ALTER TABLE node_aggregation_witness_jobs ADD COLUMN attempts INT NOT NULL DEFAULT 0;
ALTER TABLE scheduler_witness_jobs ADD COLUMN attempts INT NOT NULL DEFAULT 0;
ALTER TABLE contract_verification_requests ADD COLUMN attempts INT NOT NULL DEFAULT 0;
