ALTER TABLE contract_verification_requests RENAME COLUMN compiler_solc_version TO compiler_version;
ALTER TABLE contract_verification_requests RENAME COLUMN compiler_zksolc_version TO zk_compiler_version;
ALTER TABLE contract_verification_requests ADD COLUMN IF NOT EXISTS optimizer_mode TEXT;

DROP TABLE IF EXISTS contract_verification_zksolc_versions;
DROP TABLE IF EXISTS contract_verification_solc_versions;

CREATE TABLE IF NOT EXISTS compiler_versions (
    version TEXT NOT NULL,
    compiler TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    PRIMARY KEY (version, compiler)
);
