ALTER TABLE blocks DROP COLUMN IF EXISTS predicted_commit_gas_cost;
ALTER TABLE blocks DROP COLUMN IF EXISTS predicted_prove_gas_cost;
ALTER TABLE blocks DROP COLUMN IF EXISTS predicted_execute_gas_cost;

ALTER TABLE eth_txs DROP COLUMN IF EXISTS predicted_gas_cost;
