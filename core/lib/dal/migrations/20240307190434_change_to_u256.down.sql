ALTER table l1_batches DROP COLUMN l1_gas_price_u256;
ALTER table l1_batches DROP COLUMN l2_fair_gas_price_u256;

ALTER TABLE miniblocks DROP COLUMN l1_gas_price_u256;
ALTER TABLE miniblocks DROP COLUMN l2_fair_gas_price_u256;
ALTER TABLE miniblocks DROP COLUMN fair_pubdata_price_u256;

DROP FUNCTION udpate_l1_gas_price_old_column() cascade;
DROP FUNCTION update_l2_fair_gas_price_old_column() cascade;
DROP FUNCTION update_fair_pubdata_price_old_column() cascade;
