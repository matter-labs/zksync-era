DROP INDEX IF EXISTS storage_logs_contract_address_tx_hash_idx;
CREATE INDEX IF NOT EXISTS storage_logs_contract_address_tx_hash_idx_upd ON storage_logs (tx_hash) WHERE (address = '\x0000000000000000000000000000000000008002'::bytea);
