-- needed as we incorrectly set values of chain_id
ALTER TABLE eth_txs SET chain_id = NULL;
