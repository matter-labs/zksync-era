ALTER TABLE witness_inputs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE leaf_aggregation_witness_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE node_aggregation_witness_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE recursion_tip_witness_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE scheduler_witness_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE proof_compression_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE prover_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE prover_jobs_fri_archive ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
