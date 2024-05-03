ALTER TABLE transactions ADD COLUMN execution_info JSONB NOT NULL DEFAULT '{}';
