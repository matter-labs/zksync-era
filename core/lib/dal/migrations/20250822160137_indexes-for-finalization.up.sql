DROP INDEX IF EXISTS eth_precommit_tx_id_miniblocks;

CREATE INDEX IF NOT EXISTS eth_txs_history_finalized ON eth_txs_history ( eth_tx_id ) WHERE (finality_status = 'finalized');
CREATE INDEX IF NOT EXISTS eth_txs_history_non_finalized ON eth_txs_history ( id ) WHERE (finality_status != 'finalized');
CREATE INDEX IF NOT EXISTS eth_txs_non_gateway_index ON eth_txs ( from_addr, confirmed_eth_tx_history_id ) WHERE ( is_gateway = false and from_addr is not null);
CREATE INDEX IF NOT EXISTS eth_txs_gateway_index ON eth_txs ( from_addr, confirmed_eth_tx_history_id ) WHERE ( is_gateway = true and from_addr is not null);
CREATE INDEX IF NOT EXISTS miniblocks_precommit_txs ON miniblocks ( eth_precommit_tx_id, number );
