ALTER TABLE initial_writes ADD COLUMN IF NOT EXISTS index BIGINT;
CREATE UNIQUE INDEX IF NOT EXISTS initial_writes_index_index ON initial_writes (index);
