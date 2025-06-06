ALTER TABLE miniblocks DROP COLUMN rolling_txs_hash;
ALTER TABLE miniblocks DROP COLUMN eth_precommit_tx_id;
ALTER TABLE eth_txs DROP COLUMN status;
ALTER TABLE l1_batches DROP COLUMN final_precommit_eth_tx_id;
