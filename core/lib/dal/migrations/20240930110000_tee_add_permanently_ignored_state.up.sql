-- There were manually added tee_proof_generation_details entries with status 'permanently_ignore'.

UPDATE tee_proof_generation_details SET status = 'permanently_ignored' WHERE status = 'permanently_ignore';

-- Entries with the status 'unpicked' were not used at all after the migration to the logic
-- introduced in https://github.com/matter-labs/zksync-era/pull/3017. This was overlooked.

DELETE FROM tee_proof_generation_details WHERE status = 'unpicked';
