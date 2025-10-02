-- These requests were not supposed to be added to the DB in the first place.
-- It's been fixed, and now we can safely remove them.
DELETE FROM etherscan_verification_requests
WHERE status = 'failed' AND error = 'Unsupported source code data format';

-- We didn't process such errors properly, so we need to requeue these requests.
-- We have just a few failed requests, so the performance of the query is not a concern.
UPDATE etherscan_verification_requests
SET 
	status = 'queued',
	etherscan_verification_id = NULL,
	processing_started_at = NULL,
	error = NULL,
	attempts = 1
WHERE status = 'failed' AND error LIKE '%Unable to locate ContractCode%';
