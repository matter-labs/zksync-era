CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_status_processing_attempts_2
    ON prover_jobs_fri (processing_started_at, attempts)
    WHERE status IN ('in_progress', 'failed', 'in_gpu_proof');

DROP INDEX IF EXISTS idx_prover_jobs_fri_status_processing_attempts;
