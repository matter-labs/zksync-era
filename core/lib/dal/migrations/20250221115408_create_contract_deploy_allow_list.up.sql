-- This migration creates the `contract_deploy_allow_list` table,
-- which stores addresses that are allowed to deploy contracts.

CREATE TABLE contract_deploy_allow_list (
    address BYTEA PRIMARY KEY,
    added_at TIMESTAMPTZ DEFAULT now()  -- Timestamp when the address was added
);
