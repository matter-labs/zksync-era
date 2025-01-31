ALTER TABLE events ADD COLUMN tx_initiator_address BYTEA NOT NULL DEFAULT '\x0000000000000000000000000000000000000000'::bytea;
CREATE INDEX events_tx_initiator_address_idx ON events (tx_initiator_address);
