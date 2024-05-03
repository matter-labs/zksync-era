CREATE INDEX IF NOT EXISTS miniblocks_pending_batch ON miniblocks USING btree (number) INCLUDE (timestamp)
    WHERE l1_batch_number IS NULL;
