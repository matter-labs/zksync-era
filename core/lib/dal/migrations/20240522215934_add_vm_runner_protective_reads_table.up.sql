CREATE TABLE IF NOT EXISTS vm_runner_protective_reads
(
    l1_batch_number       BIGINT    NOT NULL PRIMARY KEY,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    time_taken            TIME
);
