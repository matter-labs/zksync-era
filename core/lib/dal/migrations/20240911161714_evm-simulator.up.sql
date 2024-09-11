ALTER TABLE protocol_versions ADD COLUMN IF NOT EXISTS evm_simulator_code_hash BYTEA;
ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS evm_simulator_code_hash BYTEA;
ALTER TABLE miniblocks ADD COLUMN IF NOT EXISTS evm_simulator_code_hash BYTEA;
