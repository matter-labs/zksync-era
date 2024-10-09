ALTER TABLE protocol_versions ADD COLUMN IF NOT EXISTS evm_emulator_code_hash BYTEA;
ALTER TABLE l1_batches ADD COLUMN IF NOT EXISTS evm_emulator_code_hash BYTEA;
-- We need this column in `miniblocks` as well in order to store data for the pending L1 batch
ALTER TABLE miniblocks ADD COLUMN IF NOT EXISTS evm_emulator_code_hash BYTEA;
