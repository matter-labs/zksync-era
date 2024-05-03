DROP INDEX IF EXISTS prover_jobs_fri_composite_index;
DROP INDEX IF EXISTS leaf_aggregation_witness_jobs_fri_composite_index;
DROP INDEX IF EXISTS node_aggregation_witness_jobs_fri_composite_index;

CREATE UNIQUE INDEX IF NOT EXISTS prover_jobs_fri_composite_index_1 ON prover_jobs_fri(l1_batch_number, aggregation_round, circuit_id, depth, sequence_number) INCLUDE(protocol_version);
CREATE UNIQUE INDEX IF NOT EXISTS leaf_aggregation_witness_jobs_fri_composite_index_1 ON leaf_aggregation_witness_jobs_fri(l1_batch_number, circuit_id) INCLUDE(protocol_version);
CREATE UNIQUE INDEX IF NOT EXISTS node_aggregation_witness_jobs_fri_composite_index_1 ON node_aggregation_witness_jobs_fri(l1_batch_number, circuit_id, depth) INCLUDE(protocol_version);
CREATE UNIQUE INDEX IF NOT EXISTS scheduler_witness_jobs_fri_composite_index_1 ON scheduler_witness_jobs_fri(l1_batch_number) INCLUDE(protocol_version);

