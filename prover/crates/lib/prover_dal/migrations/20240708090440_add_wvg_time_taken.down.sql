ALTER TABLE prover_jobs_fri
    DROP COLUMN IF EXISTS wvg_time_taken;

ALTER TABLE prover_jobs_fri_archive
    DROP COLUMN IF EXISTS wvg_time_taken;
