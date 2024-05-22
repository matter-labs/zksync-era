ALTER TABLE eth_txs ADD COLUMN chain_id BIGINT;

CREATE TABLE eth_watcher_state (
  chain_id BIGINT PRIMARY KEY,
  last_seen_block_number BIGINT
);
