DROP TABLE IF EXISTS proof_manager;
DROP TYPE IF EXISTS proving_mode;
ALTER TABLE proof_generation_details DROP COLUMN IF EXISTS proving_mode;
