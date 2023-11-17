ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS storage_refunds BIGINT[];

-- DONT LET BE ME MERGED --
ALTER TABLE miniblocks
    ADD COLUMN IF NOT EXISTS pubdata_price BIGINT;
