ALTER TABLE eth_proof_manager
    DROP COLUMN submission_started_at;

UPDATE eth_proof_manager
SET witness_inputs_url = ''
WHERE witness_inputs_url IS NULL;

ALTER TABLE eth_proof_manager
    ALTER COLUMN witness_inputs_url SET NOT NULL;
