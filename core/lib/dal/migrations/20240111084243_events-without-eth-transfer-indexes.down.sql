ALTER TABLE events
    DROP COLUMN IF EXISTS event_index_in_block_without_eth_transfer;
ALTER TABLE events
    DROP COLUMN IF EXISTS event_index_in_tx_without_eth_transfer;
