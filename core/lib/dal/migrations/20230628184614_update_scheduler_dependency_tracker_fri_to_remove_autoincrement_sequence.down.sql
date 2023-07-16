ALTER TABLE scheduler_dependency_tracker_fri
    ALTER COLUMN circuit_1_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_2_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_3_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_4_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_5_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_6_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_7_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_8_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_9_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_10_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_11_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_12_final_prover_job_id TYPE BIGSERIAL,
    ALTER COLUMN circuit_13_final_prover_job_id TYPE BIGSERIAL;

ALTER TABLE scheduler_dependency_tracker_fri
    ADD FOREIGN KEY (circuit_1_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_2_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_3_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_4_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_5_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_6_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_7_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_8_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_9_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_10_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_11_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_12_final_prover_job_id) REFERENCES prover_jobs_fri (id),
    ADD FOREIGN KEY (circuit_13_final_prover_job_id) REFERENCES prover_jobs_fri (id);
