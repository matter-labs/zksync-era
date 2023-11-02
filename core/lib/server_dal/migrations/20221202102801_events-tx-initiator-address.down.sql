DROP INDEX events_tx_initiator_address_idx;
ALTER TABLE events DROP COLUMN tx_initiator_address BYTEA;
