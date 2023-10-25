CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_queued_order
    ON prover_jobs_fri (aggregation_round DESC, l1_batch_number ASC, id ASC)
    WHERE status = 'queued';

CREATE INDEX IF NOT EXISTS idx_prover_jobs_fri_status_processing_attempts
    ON prover_jobs_fri (processing_started_at, attempts)
    WHERE status IN ('in_progress', 'failed');


CREATE INDEX IF NOT EXISTS idx_witness_inputs_fri_status_processing_attempts
    ON witness_inputs_fri (processing_started_at, attempts)
    WHERE status IN ('in_progress', 'failed');


CREATE INDEX IF NOT EXISTS idx_leaf_aggregation_witness_jobs_fri_queued_order
    ON leaf_aggregation_witness_jobs_fri (l1_batch_number ASC, id ASC)
    WHERE status = 'queued';

CREATE INDEX IF NOT EXISTS idx_leaf_aggregation_fri_status_processing_attempts
    ON leaf_aggregation_witness_jobs_fri (processing_started_at, attempts)
    WHERE status IN ('in_progress', 'failed');


CREATE INDEX IF NOT EXISTS idx_node_aggregation_witness_jobs_fri_queued_order
    ON node_aggregation_witness_jobs_fri (l1_batch_number ASC, depth ASC, id ASC)
    WHERE status = 'queued';

CREATE INDEX IF NOT EXISTS idx_node_aggregation_fri_status_processing_attempts
    ON node_aggregation_witness_jobs_fri (processing_started_at, attempts)
    WHERE status IN ('in_progress', 'failed');

CREATE INDEX IF NOT EXISTS idx_scheduler_fri_status_processing_attempts
    ON scheduler_witness_jobs_fri (processing_started_at, attempts)
    WHERE status IN ('in_progress', 'failed');

CREATE INDEX IF NOT EXISTS idx_scheduler_dependency_tracker_fri_circuit_ids_filtered
    ON scheduler_dependency_tracker_fri (
                                         circuit_1_final_prover_job_id,
                                         circuit_2_final_prover_job_id,
                                         circuit_3_final_prover_job_id,
                                         circuit_4_final_prover_job_id,
                                         circuit_5_final_prover_job_id,
                                         circuit_6_final_prover_job_id,
                                         circuit_7_final_prover_job_id,
                                         circuit_8_final_prover_job_id,
                                         circuit_9_final_prover_job_id,
                                         circuit_10_final_prover_job_id,
                                         circuit_11_final_prover_job_id,
                                         circuit_12_final_prover_job_id,
                                         circuit_13_final_prover_job_id
        ) WHERE status != 'queued';

