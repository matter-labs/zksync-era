ALTER TABLE protocol_versions DROP COLUMN IF NOT EXISTS evm_simulator_code_hash;
ALTER TABLE l1_batches DROP COLUMN IF NOT EXISTS evm_simulator_code_hash;
ALTER TABLE miniblocks DROP COLUMN IF NOT EXISTS evm_simulator_code_hash;
