UPDATE tee_proof_generation_details
SET status = 'unpicked'
WHERE status = 'ready_to_be_proven';
