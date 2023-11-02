ALTER TABLE prover_jobs_fri ADD COLUMN IF NOT EXISTS proof BYTEA;
ALTER TABLE prover_jobs_fri DROP COLUMN IF EXISTS proof_blob_url;
