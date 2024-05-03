-- -- Add up migration script here
ALTER TABLE transactions RENAME COLUMN ergs_limit TO gas_limit;
ALTER TABLE transactions RENAME COLUMN ergs_per_storage_limit TO gas_per_storage_limit;
ALTER TABLE transactions RENAME COLUMN ergs_per_pubdata_limit TO gas_per_pubdata_limit;
ALTER TABLE transactions RENAME COLUMN refunded_ergs TO refunded_gas;
ALTER TABLE transactions RENAME COLUMN max_fee_per_erg TO max_fee_per_gas;
ALTER TABLE transactions RENAME COLUMN max_priority_fee_per_erg TO max_priority_fee_per_gas;

ALTER TABLE l1_batches RENAME COLUMN ergs_per_pubdata_byte_in_block TO gas_per_pubdata_byte_in_block;
ALTER TABLE l1_batches RENAME COLUMN base_fee_per_erg TO base_fee_per_gas;
ALTER TABLE l1_batches RENAME COLUMN ergs_per_pubdata_limit TO gas_per_pubdata_limit;

ALTER TABLE l1_batches RENAME COLUMN l2_fair_ergs_price TO l2_fair_gas_price;

ALTER TABLE miniblocks RENAME COLUMN l2_fair_ergs_price TO l2_fair_gas_price;
ALTER TABLE miniblocks RENAME COLUMN base_fee_per_erg TO base_fee_per_gas;
ALTER TABLE miniblocks RENAME COLUMN ergs_per_pubdata_limit TO gas_per_pubdata_limit;



