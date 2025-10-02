CREATE TABLE IF NOT EXISTS contract_verification_info_v2 (
    initial_contract_addr BYTEA NOT NULL PRIMARY KEY,
    bytecode_keccak256 BYTEA NOT NULL,
    bytecode_without_metadata_keccak256 BYTEA NOT NULL,
    verification_info JSONB NOT NULL,

    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Add hash indexes for hash columns
CREATE INDEX IF NOT EXISTS contract_verification_info_v2_bytecode_keccak256_idx ON contract_verification_info_v2 (bytecode_keccak256);
CREATE INDEX IF NOT EXISTS contract_verification_info_v2_bytecode_without_metadata_keccak256_idx ON contract_verification_info_v2 (bytecode_without_metadata_keccak256);
