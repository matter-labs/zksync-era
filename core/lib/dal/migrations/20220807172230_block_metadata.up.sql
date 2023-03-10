ALTER TABLE blocks ADD compressed_initial_writes BYTEA;
ALTER TABLE blocks ADD compressed_repeated_writes BYTEA;
ALTER TABLE blocks ADD l2_l1_compressed_messages BYTEA;
ALTER TABLE blocks ADD l2_l1_merkle_root BYTEA;
ALTER TABLE blocks ADD ergs_per_pubdata_byte_in_block INT;
ALTER TABLE blocks ADD ergs_per_code_decommittment_word INT;
ALTER TABLE blocks ADD rollup_last_leaf_index BIGINT;
ALTER TABLE blocks ADD zkporter_is_available BOOL;
ALTER TABLE blocks ADD bootloader_code_hash BYTEA;
ALTER TABLE blocks ADD default_aa_code_hash BYTEA;
