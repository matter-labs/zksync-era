CREATE TABLE IF NOT EXISTS etherscan_verification_requests (
    contract_verification_request_id BIGINT PRIMARY KEY,
    status CHARACTER VARYING(32) NOT NULL,
    processing_started_at TIMESTAMP,
    attempts INT NOT NULL DEFAULT 0,
    etherscan_verification_id CHARACTER VARYING(128),
    error TEXT,
    retry_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    CONSTRAINT contract_verification_requests_id_fkey FOREIGN KEY (contract_verification_request_id) REFERENCES public.contract_verification_requests (id) ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE INDEX IF NOT EXISTS contract_verification_requests_status_processing_at_idx
ON contract_verification_requests (status, processing_started_at) 
WHERE status = 'queued' OR status = 'in_progress';

CREATE INDEX IF NOT EXISTS etherscan_verification_requests_status_processing_at_idx
ON etherscan_verification_requests (status, processing_started_at) 
WHERE status = 'queued' OR status = 'in_progress';
