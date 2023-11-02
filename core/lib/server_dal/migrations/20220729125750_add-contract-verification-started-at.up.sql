ALTER TABLE contract_verification_requests
    ADD COLUMN IF NOT EXISTS processing_started_at TIMESTAMP;
