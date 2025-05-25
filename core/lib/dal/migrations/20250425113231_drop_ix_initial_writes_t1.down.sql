CREATE INDEX ix_initial_writes_t1
    ON initial_writes (hashed_key) include (l1_batch_number);
