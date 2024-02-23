ALTER TABLE scheduler_dependency_tracker_fri ADD COLUMN IF NOT EXISTS circuit_255_final_prover_job_id_0 BIGINT DEFAULT NULL;
ALTER TABLE scheduler_dependency_tracker_fri ADD COLUMN IF NOT EXISTS circuit_255_final_prover_job_id_1 BIGINT DEFAULT NULL;
ALTER TABLE witness_inputs_fri ADD COLUMN blobs_4844 BYTEA;

