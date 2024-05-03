CREATE TABLE call_traces (
    tx_hash   BYTEA PRIMARY KEY,
    call_trace BYTEA NOT NULL,
    FOREIGN KEY (tx_hash) REFERENCES transactions (hash) ON DELETE CASCADE
);
