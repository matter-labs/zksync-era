ALTER TABLE l1_batches DROP COLUMN IF EXISTS state_diff_hash BYTEA;

ALTER TABLE l1_batches DROP COLUMN IF EXISTS aggregation_root;
ALTER TABLE l1_batches DROP COLUMN IF EXISTS local_root;

ALTER TABLE miniblocks
    DROP COLUMN IF EXISTS l2_da_validator_address,
    DROP COLUMN IF EXISTS pubdata_type;
