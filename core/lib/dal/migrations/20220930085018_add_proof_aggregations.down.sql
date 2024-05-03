DROP TABLE IF EXISTS leaf_aggregation_witness_jobs;
DROP TABLE IF EXISTS node_aggregation_witness_jobs;
ALTER TABLE prover_jobs DELETE COLUMN aggregation_round;

