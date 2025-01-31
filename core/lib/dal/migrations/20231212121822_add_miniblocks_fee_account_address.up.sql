ALTER TABLE miniblocks
    ADD COLUMN fee_account_address BYTEA NOT NULL DEFAULT '\x0000000000000000000000000000000000000000'::bytea;
-- ^ Add a default value so that DB queries don't fail even if the DB migration is not completed.

-- Set default values for columns in `l1_batches` that will be removed, so that INSERTs can work
-- w/o setting these columns.
ALTER TABLE l1_batches
    ALTER COLUMN fee_account_address SET DEFAULT '\x0000000000000000000000000000000000000000'::bytea,
    ALTER COLUMN is_finished SET DEFAULT true;
