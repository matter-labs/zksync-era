-- Add down migration script here
ALTER TABLE transactions RENAME COLUMN gas_limit TO ergs_limit;
ALTER TABLE transactions RENAME COLUMN gas_per_storage_limit TO ergs_per_storage_limit;
ALTER TABLE transactions RENAME COLUMN gas_per_pubdata_limit TO ergs_per_pubdata_limit;
ALTER TABLE transactions RENAME COLUMN refunded_gas TO refunded_ergs;
ALTER TABLE transactions RENAME COLUMN max_fee_per_gas TO max_fee_per_erg;
ALTER TABLE transactions RENAME COLUMN max_priority_fee_per_gas TO max_priority_fee_per_erg;

ALTER TABLE l1_batches RENAME COLUMN gas_per_pubdata_byte_in_block TO ergs_per_pubdata_byte_in_block;
ALTER TABLE l1_batches RENAME COLUMN base_fee_per_gas TO base_fee_per_erg;
ALTER TABLE l1_batches RENAME COLUMN gas_per_pubdata_limit TO ergs_per_pubdata_limit;

ALTER TABLE l1_batches RENAME COLUMN l2_fair_gas_price TO l2_fair_ergs_price;

ALTER TABLE miniblocks RENAME COLUMN l2_fair_gas_price TO l2_fair_ergs_price;
ALTER TABLE miniblocks RENAME COLUMN base_fee_per_gas TO base_fee_per_erg;
ALTER TABLE miniblocks RENAME COLUMN gas_per_pubdata_limit TO ergs_per_pubdata_limit;



