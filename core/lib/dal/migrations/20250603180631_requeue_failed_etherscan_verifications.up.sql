-- Requeue failed Etherscan verification requests after Etherscan has fixed the underlying issue.
-- We have just a few failed requests, so the performance of the query is not a concern.
UPDATE etherscan_verification_requests
SET 
	status = 'queued',
	etherscan_verification_id = NULL,
	processing_started_at = NULL,
	error = NULL,
	attempts = 0
WHERE error LIKE '%Source code exceeds max accepted (500k chars) length%'
    OR error LIKE '%Invalid or not supported solc version%'
    OR error LIKE '%Invalid zksolc version%';
