-- Handling of `Contract bytecode is not available` error has been improved.
-- Requeue all requests that failed with this error.
UPDATE etherscan_verification_requests
SET 
	status = 'queued',
	etherscan_verification_id = NULL,
	processing_started_at = NULL,
	error = NULL,
	attempts = 0
WHERE status = 'failed' AND error LIKE 'Contract bytecode is not available';
