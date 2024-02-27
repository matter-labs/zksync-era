ALTER TABLE scheduler_dependency_tracker_fri DROP COLUMN IF EXISTS eip_4844_final_prover_job_id_1;
ALTER TABLE scheduler_dependency_tracker_fri DROP COLUMN IF EXISTS eip_4844_final_prover_job_id_0;

ALTER TABLE witness_inputs_fri DROP COLUMN IF EXISTS eip_4844_blobs;
