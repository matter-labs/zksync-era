ALTER TABLE l1_batches
    RENAME COLUMN is_finished TO is_sealed;
ALTER table l1_batches
    ADD COLUMN fair_pubdata_price bigint NOT NULL DEFAULT 0,
    ADD COLUMN fee_address        bytea  NOT NULL DEFAULT '\x0000000000000000000000000000000000000000'::bytea;
