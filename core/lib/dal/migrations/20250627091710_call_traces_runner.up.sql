CREATE TABLE IF NOT EXISTS vm_runner_call_traces_runner
(
    l1_batch_number       BIGINT    NOT NULL PRIMARY KEY,
    created_at            TIMESTAMP NOT NULL,
    updated_at            TIMESTAMP NOT NULL,
    time_taken            TIME,
    processing_started_at TIMESTAMP NOT NULL
);
