CREATE TABLE l1_batch_cycle_stats (
    l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
    predicted_cycles BIGINT,
    real_cycles BIGINT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
