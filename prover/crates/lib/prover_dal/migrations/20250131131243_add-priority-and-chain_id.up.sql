ALTER TABLE witness_inputs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE leaf_aggregation_witness_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE node_aggregation_witness_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE recursion_tip_witness_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE scheduler_witness_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE proof_compression_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE prover_jobs_fri ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE prover_jobs_fri_archive ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_witness_inputs_fri_priority
    ON witness_inputs_fri USING btree (priority, created_at)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_leaf_aggregation_witness_jobs_fri_priority
    ON leaf_aggregation_witness_jobs_fri USING btree (priority, created_at)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_node_aggregation_witness_jobs_fri_priority
    ON node_aggregation_witness_jobs_fri USING btree (priority, created_at)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_recursion_tip_witness_jobs_fri_priority
    ON recursion_tip_witness_jobs_fri USING btree (priority, created_at)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_scheduler_witness_jobs_fri_priority
    ON scheduler_witness_jobs_fri USING btree (priority, created_at)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_proof_compression_jobs_fri_priority
    ON proof_compression_jobs_fri USING btree (priority, created_at)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_priority
    ON prover_jobs_fri USING btree (priority, created_at, aggregation_round, circuit_id)
    WHERE (status = 'queued'::text);
