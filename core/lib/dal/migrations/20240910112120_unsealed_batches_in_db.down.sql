ALTER TABLE l1_batches
    RENAME COLUMN is_sealed TO is_finished;
ALTER table l1_batches
    DROP COLUMN fair_pubdata_price,
    DROP COLUMN fee_address;
