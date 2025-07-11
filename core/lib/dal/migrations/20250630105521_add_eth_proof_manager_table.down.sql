DROP TABLE IF EXISTS eth_proof_manager;
DROP TYPE IF EXISTS proving_mode;
ALTER TABLE proof_generation_details DROP COLUMN IF EXISTS proving_mode;
