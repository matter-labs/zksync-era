ALTER TABLE witness_inputs_fri DROP COLUMN priority;
ALTER TABLE leaf_aggregation_witness_jobs_fri DROP COLUMN priority;
ALTER TABLE node_aggregation_witness_jobs_fri DROP COLUMN priority;
ALTER TABLE recursion_tip_witness_jobs_fri DROP COLUMN priority;
ALTER TABLE scheduler_witness_jobs_fri DROP COLUMN priority;
ALTER TABLE proof_compression_jobs_fri DROP COLUMN priority;
ALTER TABLE prover_jobs_fri DROP COLUMN priority;
ALTER TABLE prover_jobs_fri_archive DROP COLUMN priority;

DROP INDEX IF EXISTS idx_witness_inputs_fri_priority;
DROP INDEX IF EXISTS idx_leaf_aggregation_witness_jobs_fri_priority;
DROP INDEX IF EXISTS idx_node_aggregation_witness_jobs_fri_priority;
DROP INDEX IF EXISTS idx_recursion_tip_witness_jobs_fri_priority;
DROP INDEX IF EXISTS idx_scheduler_witness_jobs_fri_priority;
DROP INDEX IF EXISTS idx_proof_compression_jobs_fri_priority;
DROP INDEX IF EXISTS idx_prover_jobs_fri_priority;
