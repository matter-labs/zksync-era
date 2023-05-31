CREATE INDEX IF NOT EXISTS events_transfer_from
    ON events (topic2, miniblock_number, tx_index_in_block)
    WHERE topic1 = '\xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef';

CREATE INDEX IF NOT EXISTS events_transfer_to
    ON events (topic3, miniblock_number, tx_index_in_block)
    WHERE topic1 = '\xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef';
