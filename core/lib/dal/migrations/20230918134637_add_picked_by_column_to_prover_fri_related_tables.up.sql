ALTER TABLE witness_inputs_fri
    ADD COLUMN IF NOT EXISTS picked_by TEXT;
ALTER TABLE leaf_aggregation_witness_jobs_fri
    ADD COLUMN IF NOT EXISTS picked_by TEXT;
ALTER TABLE node_aggregation_witness_jobs_fri
    ADD COLUMN IF NOT EXISTS picked_by TEXT;
ALTER TABLE scheduler_witness_jobs_fri
    ADD COLUMN IF NOT EXISTS picked_by TEXT;
ALTER TABLE prover_jobs_fri
    ADD COLUMN IF NOT EXISTS picked_by TEXT;
ALTER TABLE proof_compression_jobs_fri
    ADD COLUMN IF NOT EXISTS picked_by TEXT;
