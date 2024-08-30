UPDATE tee_proof_generation_details
SET status = 'ready_to_be_proven'
WHERE status = 'unpicked';
