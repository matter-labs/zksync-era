CREATE TABLE IF NOT EXISTS prover_jobs
(
    id BIGSERIAL PRIMARY KEY,
    block_number BIGINT NOT NULL REFERENCES blocks (number) ON DELETE CASCADE,
    circuit_type TEXT NOT NULL,
    prover_input BYTEA NOT NULL,

    status TEXT NOT NULL,
    error TEXT,

    processing_started_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
