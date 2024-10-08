ALTER TABLE transactions ADD COLUMN block_timestamp_range_start TIMESTAMP DEFAULT NULL;
ALTER TABLE transactions ADD COLUMN block_timestamp_range_end TIMESTAMP DEFAULT NULL;