ALTER TABLE miniblocks
    ADD COLUMN fee_account_address BYTEA NOT NULL DEFAULT '\x0000000000000000000000000000000000000000'::bytea;
-- Add a default value so that DB queries don't fail even if the DB migration is not completed.

UPDATE miniblocks SET (fee_account_address) = (
    SELECT l1_batches.fee_account_address
    FROM l1_batches
    WHERE l1_batches.number = miniblocks.l1_batch_number
);
