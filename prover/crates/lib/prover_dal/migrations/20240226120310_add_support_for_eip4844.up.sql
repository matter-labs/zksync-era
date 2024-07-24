ALTER TABLE witness_inputs_fri ADD COLUMN IF NOT EXISTS eip_4844_blobs BYTEA;

ALTER TABLE scheduler_dependency_tracker_fri ADD COLUMN IF NOT EXISTS eip_4844_final_prover_job_id_0 BIGINT;
ALTER TABLE scheduler_dependency_tracker_fri ADD COLUMN IF NOT EXISTS eip_4844_final_prover_job_id_1 BIGINT;
