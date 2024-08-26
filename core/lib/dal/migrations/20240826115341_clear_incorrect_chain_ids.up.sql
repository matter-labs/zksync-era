-- needed as we incorrectly set values of chain_id
UPDATE eth_txs SET chain_id = NULL;
