CREATE INDEX IF NOT EXISTS prover_jobs_fri_status_processing_started_at_idx_2 ON prover_jobs_fri (status, processing_started_at)
    WHERE (attempts < 20);

DROP INDEX IF EXISTS idx_prover_jobs_fri_status_processing_attempts;

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_status ON prover_jobs_fri (circuit_id, aggregation_round)
    WHERE (status != 'successful' and status != 'skipped');

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_circuit_id_agg_batch_num
    ON prover_jobs_fri (circuit_id, aggregation_round, l1_batch_number)
    WHERE status IN ('queued', 'in_progress', 'in_gpu_proof', 'failed');
