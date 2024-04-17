ALTER TABLE contract_verification_requests ADD COLUMN IF NOT EXISTS force_evmla BOOLEAN NOT NULL DEFAULT false;
