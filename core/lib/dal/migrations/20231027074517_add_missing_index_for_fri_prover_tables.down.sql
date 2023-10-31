CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_status_processing_attempts
    ON prover_jobs_fri (processing_started_at, attempts)
    WHERE status IN ('in_progress', 'failed');

DROP INDEX IF EXISTS prover_jobs_fri_status_processing_started_at_idx_2;

DROP INDEX IF EXISTS idx_prover_jobs_fri_status;

DROP INDEX IF EXISTS idx_prover_jobs_fri_circuit_id_agg_batch_num;
