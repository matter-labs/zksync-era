ALTER TABLE contract_verification_requests
    ADD COLUMN IF NOT EXISTS compilation_errors JSONB;
