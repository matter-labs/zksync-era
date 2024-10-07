ALTER TABLE protocol_versions DROP COLUMN IF EXISTS evm_emulator_code_hash;
ALTER TABLE l1_batches DROP COLUMN IF EXISTS evm_emulator_code_hash;
ALTER TABLE miniblocks DROP COLUMN IF EXISTS evm_emulator_code_hash;
