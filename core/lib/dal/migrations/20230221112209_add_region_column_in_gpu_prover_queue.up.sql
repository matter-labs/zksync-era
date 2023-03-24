ALTER TABLE gpu_prover_queue
    ADD COLUMN IF NOT EXISTS region TEXT
    NOT NULL
    DEFAULT('prior to enabling multi-region');
