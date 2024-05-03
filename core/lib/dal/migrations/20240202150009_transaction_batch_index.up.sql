CREATE INDEX IF NOT EXISTS transactions_l1_batch_number_idx ON transactions (l1_batch_number);
CREATE INDEX IF NOT EXISTS aggregated_proof_from_l1_batch_index ON aggregated_proof (from_block_number);
CREATE INDEX IF NOT EXISTS aggregated_proof_to_l1_batch_index ON aggregated_proof (to_block_number);
