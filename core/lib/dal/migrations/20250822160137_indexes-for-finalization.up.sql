CREATE INDEX IF NOT EXISTS eth_txs_finalized_tx
  ON eth_txs_history (eth_tx_id);
