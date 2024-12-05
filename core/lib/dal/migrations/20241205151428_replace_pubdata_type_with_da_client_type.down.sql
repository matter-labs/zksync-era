ALTER TABLE miniblocks
    DROP COLUMN IF EXISTS da_client_type,
    ADD COLUMN IF NOT EXISTS pubdata_type TEXT NOT NULL DEFAULT 'Rollup';
