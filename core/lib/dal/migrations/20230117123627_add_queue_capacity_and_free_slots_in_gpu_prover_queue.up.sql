ALTER TABLE gpu_prover_queue
    ADD COLUMN IF NOT EXISTS queue_free_slots INTEGER NOT NULL,
    ADD COLUMN IF NOT EXISTS queue_capacity   INTEGER NOT NULL;
