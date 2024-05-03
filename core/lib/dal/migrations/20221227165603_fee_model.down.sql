ALTER TABLE miniblocks DROP COLUMN l1_gas_price;
ALTER TABLE miniblocks DROP COLUMN l2_fair_ergs_price;

ALTER TABLE l1_batches DROP COLUMN l1_gas_price;
ALTER TABLE l1_batches DROP COLUMN l2_fair_ergs_price;

ALTER TABLE transactions DROP COLUMN refunded_ergs;

-- ALTER TABLE miniblocks ADD COLUMN ergs_per_pubdata_limit BIGINT NOT NULL DEFAULT 0;
-- ALTER TABLE l1_batches ADD COLUMN ergs_per_pubdata_limit BIGINT NOT NULL DEFAULT 0;
