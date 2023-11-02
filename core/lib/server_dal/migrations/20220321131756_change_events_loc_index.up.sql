DROP INDEX events_tx_location_idx;
CREATE INDEX events_block_idx ON events (block_number);
