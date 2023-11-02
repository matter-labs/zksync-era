DROP TABLE tokens;

DROP INDEX transactions_block_number_idx;
DROP TABLE transactions;

DROP TABLE storage;

DROP INDEX contracts_block_number_idx;
DROP INDEX contracts_tx_hash_idx;
DROP TABLE contracts;

DROP TABLE proof;
DROP TABLE aggregated_proof;

DROP INDEX storage_logs_block_number_idx;
DROP INDEX storage_logs_raw_key_block_number_idx;
DROP TABLE storage_logs;

DROP TABLE contract_sources;
DROP TABLE transaction_traces;

DROP INDEX events_tx_location_idx;
DROP INDEX events_address_idx;
DROP INDEX events_topic1_idx;
DROP INDEX events_topic2_idx;
DROP INDEX events_topic3_idx;
DROP INDEX events_topic4_idx;
DROP INDEX events_tx_hash_idx;
DROP TABLE events;

DROP TABLE factory_deps;

DROP INDEX blocks_eth_commit_tx_id_idx;
DROP INDEX blocks_eth_execute_tx_id_idx;
DROP INDEX blocks_eth_prove_tx_id_idx;
DROP TABLE blocks;

DROP TABLE eth_txs_history;
DROP TABLE eth_txs
