ALTER TABLE miniblocks
    ADD COLUMN IF NOT EXISTS logs_bloom BYTEA;
