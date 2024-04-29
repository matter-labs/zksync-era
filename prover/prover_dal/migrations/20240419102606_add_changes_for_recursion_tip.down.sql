CREATE TABLE IF NOT EXISTS scheduler_dependency_tracker_fri (
    l1_batch_number                BIGINT NOT NULL PRIMARY KEY,
    status                         TEXT NOT NULL,
    circuit_1_final_prover_job_id  BIGINT,
    circuit_2_final_prover_job_id  BIGINT,
    circuit_3_final_prover_job_id  BIGINT,
    circuit_4_final_prover_job_id  BIGINT,
    circuit_5_final_prover_job_id  BIGINT,
    circuit_6_final_prover_job_id  BIGINT,
    circuit_7_final_prover_job_id  BIGINT,
    circuit_8_final_prover_job_id  BIGINT,
    circuit_9_final_prover_job_id  BIGINT,
    circuit_10_final_prover_job_id BIGINT,
    circuit_11_final_prover_job_id BIGINT,
    circuit_12_final_prover_job_id BIGINT,
    circuit_13_final_prover_job_id BIGINT,
    created_at                     TIMESTAMP NOT NULL,
    updated_at                     TIMESTAMP NOT NULL
);

COMMENT ON TABLE scheduler_dependency_tracker_fri IS 'Used to track all node completions, before we can queue the scheduler.';

CREATE INDEX IF NOT EXISTS idx_scheduler_dependency_tracker_fri_circuit_ids_filtered
    ON public.scheduler_dependency_tracker_fri (circuit_1_final_prover_job_id, circuit_2_final_prover_job_id, circuit_3_final_prover_job_id, circuit_4_final_prover_job_id, circuit_5_final_prover_job_id, circuit_6_final_prover_job_id, circuit_7_final_prover_job_id, circuit_8_final_prover_job_id, circuit_9_final_prover_job_id, circuit_10_final_prover_job_id, circuit_11_final_prover_job_id, circuit_12_final_prover_job_id, circuit_13_final_prover_job_id)
    WHERE (status <> 'queued'::text);

DROP INDEX IF EXISTS recursion_tip_witness_jobs_fri_status_processing_attempts_idx;

DROP TABLE IF  EXISTS recursion_tip_witness_jobs_fri;

DROP INDEX IF  EXISTS prover_jobs_fri_l1_batch_number_is_node_final_proof_true_status_successful_idx;
