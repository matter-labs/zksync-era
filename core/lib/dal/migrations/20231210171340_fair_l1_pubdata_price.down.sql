-- Add down migration script here

ALTER TABLE miniblocks
    REMOVE COLUMN l1_fair_pubdata_price;
