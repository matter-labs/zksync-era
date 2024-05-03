ALTER TABLE gpu_prover_queue
    ADD COLUMN IF NOT EXISTS zone TEXT NOT NULL DEFAULT('prior to enabling multi-zone'),
    ADD COLUMN IF NOT EXISTS num_gpu SMALLINT NOT NULL DEFAULT 2;