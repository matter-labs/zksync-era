CREATE TABLE IF NOT EXISTS contract_verification_requests (
    id BIGSERIAL PRIMARY KEY,
    contract_address BYTEA NOT NULL,
    source_code TEXT NOT NULL,
    contract_name TEXT NOT NULL,
    compiler_version TEXT NOT NULL,
    optimization_used BOOLEAN NOT NULL,
    constructor_arguments BYTEA NOT NULL,

    status TEXT NOT NULL,
    error TEXT,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

ALTER TABLE contracts ADD COLUMN IF NOT EXISTS verification_info JSONB;
CREATE INDEX IF NOT EXISTS contract_verification_requests_queued_idx
    ON contract_verification_requests (created_at) WHERE status = 'queued';
