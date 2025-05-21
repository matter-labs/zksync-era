ALTER TABLE eth_txs_history ADD COLUMN finality_status TEXT;
UPDATE eth_txs_history SET finality_status = 'finalized' WHERE finality_status IS NULL AND confirmed_at IS NOT NULL;