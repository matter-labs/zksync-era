ALTER TABLE witness_inputs_fri DROP COLUMN priority;
ALTER TABLE leaf_aggregation_witness_jobs_fri DROP COLUMN priority;
ALTER TABLE node_aggregation_witness_jobs_fri DROP COLUMN priority;
ALTER TABLE recursion_tip_witness_jobs_fri DROP COLUMN priority;
ALTER TABLE scheduler_witness_jobs_fri DROP COLUMN priority;
ALTER TABLE proof_compression_jobs_fri DROP COLUMN priority;
ALTER TABLE prover_jobs_fri DROP COLUMN priority;
ALTER TABLE prover_jobs_fri_archive DROP COLUMN priority;
