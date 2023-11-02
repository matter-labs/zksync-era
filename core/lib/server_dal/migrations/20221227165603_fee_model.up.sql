ALTER TABLE miniblocks ADD COLUMN l1_gas_price BIGINT NOT NULL DEFAULT 0;
ALTER TABLE miniblocks ADD COLUMN l2_fair_ergs_price BIGINT NOT NULL DEFAULT 0;

ALTER TABLE l1_batches ADD COLUMN l1_gas_price BIGINT NOT NULL DEFAULT 0;
ALTER TABLE l1_batches ADD COLUMN l2_fair_ergs_price BIGINT NOT NULL DEFAULT 0;

ALTER TABLE transactions ADD COLUMN refunded_ergs BIGINT NOT NULL DEFAULT 0;

-- ALTER TABLE miniblocks DROP COLUMN ergs_per_pubdata_limit;
-- ALTER TABLE l1_batches DROP COLUMN ergs_per_pubdata_limit;
-- ALTER TABLE l1_batches DROP COLUMN ergs_per_pubdata_byte_in_block;
