ALTER TABLE miniblocks
    DROP COLUMN IF EXISTS pubdata_type,
    ADD COLUMN IF NOT EXISTS da_client_type TEXT NULL;
