DROP INDEX IF EXISTS prover_jobs_fri_composite_index_1;
DROP INDEX IF EXISTS leaf_aggregation_witness_jobs_fri_composite_index_1;
DROP INDEX IF EXISTS node_aggregation_witness_jobs_fri_composite_index_1;
DROP INDEX IF EXISTS scheduler_witness_jobs_fri_composite_index_1;

CREATE UNIQUE INDEX IF NOT EXISTS prover_jobs_fri_composite_index ON prover_jobs_fri(l1_batch_number, aggregation_round, circuit_id, depth, sequence_number);
CREATE UNIQUE INDEX IF NOT EXISTS leaf_aggregation_witness_jobs_fri_composite_index ON leaf_aggregation_witness_jobs_fri(l1_batch_number, circuit_id);
CREATE UNIQUE INDEX IF NOT EXISTS node_aggregation_witness_jobs_fri_composite_index ON node_aggregation_witness_jobs_fri(l1_batch_number, circuit_id, depth);
