ALTER TABLE prover_jobs_fri
    ADD COLUMN IF NOT EXISTS wvg_time_taken TIME;

ALTER TABLE prover_jobs_fri_archive
    ADD COLUMN IF NOT EXISTS wvg_time_taken TIME;
