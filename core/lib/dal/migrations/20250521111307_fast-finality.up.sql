ALTER TABLE eth_txs ADD COLUMN finality_status TEXT;
UPDATE eth_txs SET finality_status = 'finalized' WHERE finality_status IS NULL AND confirmed_eth_tx_history_id IS NOT NULL;