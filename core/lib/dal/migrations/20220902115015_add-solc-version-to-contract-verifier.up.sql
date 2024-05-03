ALTER TABLE contract_verification_requests RENAME COLUMN compiler_version TO compiler_zksolc_version;
ALTER TABLE contract_verification_requests ADD COLUMN compiler_solc_version TEXT NOT NULL DEFAULT '0.8.16';
