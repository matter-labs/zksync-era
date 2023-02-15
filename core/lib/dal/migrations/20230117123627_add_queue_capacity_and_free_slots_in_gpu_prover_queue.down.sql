ALTER TABLE gpu_prover_queue
    DROP COLUMN IF EXISTS queue_free_slots,
    DROP COLUMN IF EXISTS queue_capacity;
