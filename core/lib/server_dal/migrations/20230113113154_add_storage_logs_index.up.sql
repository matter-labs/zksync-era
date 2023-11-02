-- This is the ACCOUNT_CODE_STORAGE address.
CREATE INDEX storage_logs_contract_address_tx_hash_idx ON storage_logs (address, tx_hash) WHERE (address = '\x0000000000000000000000000000000000008002');
