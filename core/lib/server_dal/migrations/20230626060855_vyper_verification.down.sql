DROP TABLE IF EXISTS compiler_versions;

ALTER TABLE contract_verification_requests DROP COLUMN IF EXISTS optimizer_mode;
ALTER TABLE contract_verification_requests RENAME COLUMN compiler_version TO compiler_solc_version;
ALTER TABLE contract_verification_requests RENAME COLUMN zk_compiler_version TO compiler_zksolc_version;

CREATE TABLE IF NOT EXISTS contract_verification_zksolc_versions (
    version TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
CREATE TABLE IF NOT EXISTS contract_verification_solc_versions (
    version TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
