ALTER TABLE eth_txs_history ADD COLUMN finality_status TEXT NOT NULL DEFAULT 'pending';
UPDATE eth_txs_history SET finality_status = 'finalized' WHERE finality_status = 'pending' AND confirmed_at IS NOT NULL;
