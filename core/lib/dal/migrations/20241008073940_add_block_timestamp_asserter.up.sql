ALTER TABLE transactions
ADD COLUMN timestamp_asserter_range_start TIMESTAMP DEFAULT NULL,
ADD COLUMN timestamp_asserter_range_end TIMESTAMP DEFAULT NULL;
