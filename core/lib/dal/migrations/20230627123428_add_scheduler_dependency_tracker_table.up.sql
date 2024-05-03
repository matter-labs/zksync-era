CREATE TABLE IF NOT EXISTS scheduler_dependency_tracker_fri
(
    l1_batch_number BIGINT PRIMARY KEY,
    status TEXT NOT NULL,
    circuit_1_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_2_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_3_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_4_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_5_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_6_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_7_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_8_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_9_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_10_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_11_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_12_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    circuit_13_final_prover_job_id BIGSERIAL REFERENCES prover_jobs_fri (id) ON DELETE CASCADE,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);