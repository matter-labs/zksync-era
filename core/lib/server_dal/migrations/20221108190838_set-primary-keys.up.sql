ALTER TABLE contract_verification_zksolc_versions ADD CONSTRAINT contract_verification_zksolc_versions_pkey PRIMARY KEY ( version );
ALTER TABLE contract_verification_solc_versions ADD CONSTRAINT contract_verification_solc_versions_pkey PRIMARY KEY ( version );
ALTER TABLE witness_inputs ADD CONSTRAINT witness_inputs_pkey PRIMARY KEY (l1_batch_number);
DROP INDEX witness_inputs_block_number_idx;
ALTER TABLE witness_inputs DROP CONSTRAINT unique_witnesses;
