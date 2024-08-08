ALTER TABLE miniblocks
    ADD COLUMN l2_da_validator_address BYTEA NOT NULL DEFAULT '\x0000000000000000000000000000000000000000'::bytea,
    -- There are miniblocks that used the `Rollup' type, but were actually used on a Validium chain. 
    -- This is okay, since this field represents how the VM works with the DA, rather what is committed on L1.
    ADD COLUMN pubdata_type TEXT NOT NULL DEFAULT 'Rollup';
-- ^ Add a default value so that DB queries don't fail even if the DB migration is not completed.

-- Set default values for columns in `l1_batches` that will be removed, so that INSERTs can work
-- w/o setting these columns.
-- ALTER TABLE l1_batches
--     ALTER COLUMN l2_da_validator_address SET DEFAULT '\x0000000000000000000000000000000000000000'::bytea,
--     ALTER COLUMN is_finished SET DEFAULT true;
