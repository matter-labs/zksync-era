ALTER TABLE blocks DROP compressed_repeated_writes;
ALTER TABLE blocks DROP compressed_initial_writes;
ALTER TABLE blocks DROP l2_l1_compressed_messages;
ALTER TABLE blocks DROP l2_l1_merkle_root;
ALTER TABLE blocks DROP l2_l1_linear_hash;
ALTER TABLE blocks DROP ergs_per_pubdata_byte_in_block;
ALTER TABLE blocks DROP ergs_per_code_decommittment_word;
ALTER TABLE blocks DROP rollup_last_leaf_index;
ALTER TABLE blocks DROP zkporter_is_available;
ALTER TABLE blocks DROP bootloader_code_hash;
ALTER TABLE blocks DROP default_aa_code_hash;
