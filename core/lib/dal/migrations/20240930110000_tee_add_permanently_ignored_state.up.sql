-- there were manually added tee_proof_generation_details with status 'permanently_ignore'
UPDATE tee_proof_generation_details SET status = 'permanently_ignored' WHERE status = 'permanently_ignore';
