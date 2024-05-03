CREATE INDEX IF NOT EXISTS transactions_block_number_tx_index ON transactions (block_number, index_in_block);
CREATE INDEX IF NOT EXISTS events_block_number_tx_index ON events (block_number, tx_index_in_block);
