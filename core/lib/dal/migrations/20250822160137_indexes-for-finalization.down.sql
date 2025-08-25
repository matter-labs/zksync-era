DROP INDEX IF EXIST eth_txs_history_finalized;
DROP INDEX IF EXIST eth_txs_history_non_finalized;
DROP INDEX IF EXIST eth_txs_non_gateway_index;
DROP INDEX IF EXIST eth_txs_gateway_index;
DROP INDEX IF EXIST miniblocks_precommit_txs;
CREATE INDEX IF NOT EXISTS eth_precommit_tx_id_miniblocks ON miniblocks (eth_precommit_tx_id);
