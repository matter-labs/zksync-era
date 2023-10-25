ALTER TABLE gpu_prover_queue
DROP COLUMN IF EXISTS zone,
    DROP COLUMN IF EXISTS num_gpu;
