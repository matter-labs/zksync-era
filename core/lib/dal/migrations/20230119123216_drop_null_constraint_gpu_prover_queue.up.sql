ALTER TABLE gpu_prover_queue ALTER COLUMN queue_capacity DROP NOT NULL;
ALTER TABLE gpu_prover_queue ALTER COLUMN queue_free_slots DROP NOT NULL;
