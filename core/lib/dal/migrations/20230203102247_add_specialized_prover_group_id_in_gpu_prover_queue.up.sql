ALTER TABLE gpu_prover_queue
    ADD COLUMN IF NOT EXISTS specialized_prover_group_id SMALLINT;
