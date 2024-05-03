DELETE FROM eth_txs_history
WHERE confirmed_at IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM eth_txs WHERE eth_txs.confirmed_eth_tx_history_id = eth_txs_history.id);

CREATE UNIQUE INDEX IF NOT EXISTS eth_txs_history_tx_hash_index ON eth_txs_history (tx_hash);
