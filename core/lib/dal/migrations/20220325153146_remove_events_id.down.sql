ALTER TABLE events DROP CONSTRAINT events_pkey;
ALTER TABLE events ADD COLUMN id SERIAL PRIMARY KEY;

CREATE INDEX events_block_idx ON events (block_number);
