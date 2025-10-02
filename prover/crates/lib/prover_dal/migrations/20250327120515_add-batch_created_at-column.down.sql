ALTER TABLE witness_inputs_fri DROP COLUMN batch_sealed_at;
ALTER TABLE leaf_aggregation_witness_jobs_fri DROP COLUMN batch_sealed_at;
ALTER TABLE node_aggregation_witness_jobs_fri DROP COLUMN batch_sealed_at;
ALTER TABLE recursion_tip_witness_jobs_fri DROP COLUMN batch_sealed_at;
ALTER TABLE scheduler_witness_jobs_fri DROP COLUMN batch_sealed_at;
ALTER TABLE proof_compression_jobs_fri DROP COLUMN batch_sealed_at;
ALTER TABLE prover_jobs_fri DROP COLUMN batch_sealed_at;
ALTER TABLE prover_jobs_fri_archive DROP COLUMN batch_sealed_at;

DROP INDEX IF EXISTS idx_witness_inputs_fri_priority;
DROP INDEX IF EXISTS idx_leaf_aggregation_witness_jobs_fri_priority;
DROP INDEX IF EXISTS idx_node_aggregation_witness_jobs_fri_priority;
DROP INDEX IF EXISTS idx_recursion_tip_witness_jobs_fri_priority;
DROP INDEX IF EXISTS idx_scheduler_witness_jobs_fri_priority;
DROP INDEX IF EXISTS idx_proof_compression_jobs_fri_priority;
DROP INDEX IF EXISTS idx_prover_jobs_fri_priority;

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
