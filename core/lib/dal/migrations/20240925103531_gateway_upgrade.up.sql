ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS state_diff_hash BYTEA;

ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS aggregation_root BYTEA;
ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS local_root BYTEA;

ALTER TABLE miniblocks
    ADD COLUMN IF NOT EXISTS l2_da_validator_address BYTEA NOT NULL DEFAULT '\x0000000000000000000000000000000000000000'::bytea,
    -- There are miniblocks that used the `Rollup' type, but were actually used on a Validium chain.
    -- This is okay, since this field represents how the VM works with the DA, rather what is committed on L1.
    ADD COLUMN IF NOT EXISTS pubdata_type TEXT NOT NULL DEFAULT 'Rollup';
-- ^ Add a default value so that DB queries don't fail even if the DB migration is not completed.
