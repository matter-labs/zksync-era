CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_get_next_job
    ON prover_jobs_fri USING btree (l1_batch_number, aggregation_round, circuit_id, id)
    WHERE (status = 'queued'::text);

DROP INDEX IF EXISTS idx_prover_jobs_fri_circuit_id_agg_batch_num;

DROP INDEX IF EXISTS idx_prover_jobs_fri_queued_order;

DROP INDEX IF EXISTS idx_prover_jobs_fri_queued_order2;

DROP INDEX IF EXISTS idx_prover_jobs_fri_status;

DROP INDEX IF EXISTS ix_prover_jobs_fri_t1;

