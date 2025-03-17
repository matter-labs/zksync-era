CREATE INDEX IF NOT EXISTS ix_prover_jobs_fri_t1
    ON prover_jobs_fri (circuit_id, aggregation_round, l1_batch_number, id)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_status
    ON prover_jobs_fri (circuit_id, aggregation_round, status)
    WHERE ((status <> 'successful'::text) AND (status <> 'skipped'::text));

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_queued_order2
    ON prover_jobs_fri (l1_batch_number, aggregation_round DESC, id)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_queued_order
    ON prover_jobs_fri (aggregation_round DESC, l1_batch_number, id)
    WHERE (status = 'queued'::text);

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_circuit_id_agg_batch_num
    ON prover_jobs_fri (circuit_id, aggregation_round, l1_batch_number)
    WHERE (status = ANY (ARRAY['queued'::text, 'in_progress'::text, 'in_gpu_proof'::text, 'failed'::text]));

DROP INDEX IF EXISTS idx_prover_jobs_fri_get_next_job;
