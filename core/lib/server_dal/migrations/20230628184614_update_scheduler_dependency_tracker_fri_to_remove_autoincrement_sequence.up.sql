ALTER TABLE scheduler_dependency_tracker_fri
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_1_final_prover_job_i_fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_2_final_prover_job_i_fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_3_final_prover_job_i_fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_4_final_prover_job_i_fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_5_final_prover_job_i_fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_6_final_prover_job_i_fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_7_final_prover_job_i_fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_8_final_prover_job_i_fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_9_final_prover_job_i_fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_10_final_prover_job__fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_11_final_prover_job__fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_12_final_prover_job__fkey,
    DROP CONSTRAINT IF EXISTS scheduler_dependency_tracker__circuit_13_final_prover_job__fkey;

ALTER TABLE scheduler_dependency_tracker_fri
    ALTER COLUMN circuit_1_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_2_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_3_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_4_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_5_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_6_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_7_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_8_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_9_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_10_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_11_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_12_final_prover_job_id DROP NOT NULL,
    ALTER COLUMN circuit_13_final_prover_job_id DROP NOT NULL;

DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_1_final_prover_job_id_seq CASCADE;
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_2_final_prover_job_id_seq CASCADE;
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_3_final_prover_job_id_seq CASCADE;
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_4_final_prover_job_id_seq CASCADE;
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_5_final_prover_job_id_seq CASCADE;
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_6_final_prover_job_id_seq CASCADE;
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_7_final_prover_job_id_seq CASCADE;
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_8_final_prover_job_id_seq CASCADE;
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_9_final_prover_job_id_seq CASCADE;
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_10_final_prover_job_i_seq CASCADE; -- it not job_id because postgres truncated more than 63 chars.
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_11_final_prover_job_i_seq CASCADE; -- it not job_id because postgres truncated more than 63 chars.
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_12_final_prover_job_i_seq CASCADE; -- it not job_id because postgres truncated more than 63 chars.
DROP SEQUENCE IF EXISTS scheduler_dependency_tracker__circuit_13_final_prover_job_i_seq CASCADE; -- it not job_id because postgres truncated more than 63 chars.