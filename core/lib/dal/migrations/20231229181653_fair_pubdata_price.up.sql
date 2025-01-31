ALTER TABLE miniblocks
    ADD COLUMN IF NOT EXISTS fair_pubdata_price BIGINT;
