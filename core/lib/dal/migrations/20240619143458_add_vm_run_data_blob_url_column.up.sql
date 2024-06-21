ALTER TABLE proof_generation_details
 ADD COLUMN IF NOT EXISTS vm_run_data_blob_url TEXT DEFAULT NULL;

CREATE TABLE IF NOT EXISTS vm_runner_bwip
(
    l1_batch_number       BIGINT    NOT NULL PRIMARY KEY,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    time_taken            TIME
);
