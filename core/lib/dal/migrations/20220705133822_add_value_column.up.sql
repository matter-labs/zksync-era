ALTER TABLE transactions ADD COLUMN IF NOT EXISTS value NUMERIC(80) NOT NULL DEFAULT 0;
