DROP INDEX IF EXISTS interop_roots_processed_block_number_idx;
DROP TABLE IF EXISTS interop_roots;
ALTER TABLE l1_batches DROP COLUMN batch_chain_merkle_path_until_msg_root;
