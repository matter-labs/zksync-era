ALTER TABLE events DROP CONSTRAINT events_pkey;
ALTER TABLE events DROP COLUMN id;
ALTER TABLE events ADD PRIMARY KEY (block_number, event_index_in_block);

DROP INDEX events_block_idx;
