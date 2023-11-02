ALTER TABLE contracts DROP COLUMN IF EXISTS verification_info;
DROP TABLE IF EXISTS contract_verification_requests;
DROP INDEX IF EXISTS contract_verification_requests_queued_idx;
