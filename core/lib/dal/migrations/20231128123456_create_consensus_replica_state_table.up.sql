CREATE TABLE IF NOT EXISTS consensus_replica_state (
    state JSONB NOT NULL,
    -- artificial primary key ensuring that the table contains at most 1 row.
    fake_key BOOLEAN PRIMARY KEY,
    CHECK (fake_key)
);
