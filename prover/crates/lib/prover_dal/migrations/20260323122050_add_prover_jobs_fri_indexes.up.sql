CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_active_stats
    ON prover_jobs_fri (status) WHERE protocol_version IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_unproved
    ON prover_jobs_fri (circuit_id, aggregation_round, l1_batch_number)
    WHERE status IN ('queued', 'in_progress', 'in_gpu_proof', 'failed');

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_attempts_check
    ON prover_jobs_fri (attempts)
    WHERE status <> 'successful';

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_archive_candidates
    ON prover_jobs_fri (updated_at)
    WHERE status NOT IN ('queued', 'in_progress', 'in_gpu_proof', 'failed');
