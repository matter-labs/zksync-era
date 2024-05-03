ALTER TABLE witness_inputs_fri
    DROP COLUMN IF EXISTS picked_by;
ALTER TABLE leaf_aggregation_witness_jobs_fri
    DROP COLUMN IF EXISTS picked_by;
ALTER TABLE node_aggregation_witness_jobs_fri
    DROP COLUMN IF EXISTS picked_by;
ALTER TABLE scheduler_witness_jobs_fri
    DROP COLUMN IF EXISTS picked_by;
ALTER TABLE prover_jobs_fri
    DROP COLUMN IF EXISTS picked_by;
ALTER TABLE proof_compression_jobs_fri
    DROP COLUMN IF EXISTS picked_by;
