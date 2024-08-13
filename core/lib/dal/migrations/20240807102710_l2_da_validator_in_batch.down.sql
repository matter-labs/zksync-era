-- ALTER TABLE l1_batches
--     ALTER COLUMN fee_account_address DROP DEFAULT,
--     ALTER COLUMN is_finished DROP DEFAULT;
ALTER TABLE miniblocks
    DROP COLUMN IF EXISTS fee_account_address;
