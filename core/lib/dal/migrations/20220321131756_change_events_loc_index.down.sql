CREATE INDEX events_tx_location_idx ON events (block_number, tx_index_in_block);
DROP INDEX events_block_idx;
