CREATE INDEX IF NOT EXISTS miniblocks_number_precommit_not_null ON miniblocks ( number )
WHERE eth_precommit_tx_id IS NOT NULL;
