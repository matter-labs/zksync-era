-- `events.tx_initiator_address` was only used in Block Explorer queries that were removed from the codebase.
DROP INDEX IF EXISTS events_tx_initiator_address_idx;
ALTER TABLE events DROP COLUMN tx_initiator_address;
