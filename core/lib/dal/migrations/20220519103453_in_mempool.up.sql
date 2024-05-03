ALTER TABLE transactions ADD COLUMN IF NOT EXISTS in_mempool BOOLEAN NOT NULL default false;
CREATE INDEX IF NOT EXISTS transactions_in_mempool_idx ON transactions (in_mempool) WHERE in_mempool = TRUE;
