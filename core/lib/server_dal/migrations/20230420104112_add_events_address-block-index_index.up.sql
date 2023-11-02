CREATE INDEX IF NOT EXISTS events_address_block_event_index_in_block_index ON events (address, miniblock_number, event_index_in_block);
