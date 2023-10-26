ALTER TABLE l1_batches RENAME TO blocks;

ALTER TABLE proof RENAME CONSTRAINT proof_l1_batch_number_fkey TO proof_block_number_fkey;
ALTER TABLE proof RENAME COLUMN l1_batch_number TO block_number;

ALTER TABLE prover_jobs RENAME CONSTRAINT prover_jobs_l1_batch_number_fkey TO prover_jobs_block_number_fkey;
ALTER TABLE prover_jobs RENAME COLUMN l1_batch_number TO block_number;

ALTER TABLE witness_inputs RENAME CONSTRAINT witness_inputs_l1_batch_number_fkey TO witness_inputs_block_number_fkey;
ALTER TABLE witness_inputs RENAME COLUMN l1_batch_number TO block_number;

ALTER TABLE storage_logs_dedup RENAME CONSTRAINT storage_logs_dedup_l1_batch_number_fkey TO storage_logs_dedup_block_number_fkey;
ALTER TABLE storage_logs_dedup RENAME COLUMN l1_batch_number TO block_number;

ALTER TABLE storage_logs DROP CONSTRAINT storage_logs_miniblock_number_fkey;
ALTER TABLE storage_logs RENAME COLUMN miniblock_number TO block_number;
ALTER TABLE storage_logs ADD CONSTRAINT storage_logs_block_number_fkey
    FOREIGN KEY (block_number) REFERENCES blocks (number);

ALTER TABLE factory_deps DROP CONSTRAINT factory_deps_miniblock_number_fkey;
ALTER TABLE factory_deps RENAME COLUMN miniblock_number TO block_number;
ALTER TABLE factory_deps ADD CONSTRAINT factory_deps_block_number_fkey
    FOREIGN KEY (block_number) REFERENCES blocks (number);

ALTER TABLE events DROP CONSTRAINT events_miniblock_number_fkey;
ALTER TABLE events RENAME COLUMN miniblock_number TO block_number;
ALTER TABLE events ADD CONSTRAINT events_block_number_fkey
    FOREIGN KEY (block_number) REFERENCES blocks (number);

ALTER TABLE contracts DROP CONSTRAINT contracts_miniblock_number_fkey;
ALTER TABLE contracts RENAME COLUMN miniblock_number TO block_number;
ALTER TABLE contracts ADD CONSTRAINT contracts_block_number_fkey
    FOREIGN KEY (block_number) REFERENCES blocks (number);

ALTER TABLE transactions RENAME CONSTRAINT transactions_l1_batch_number_fkey TO transactions_block_number_fkey;
ALTER TABLE transactions RENAME COLUMN l1_batch_number TO block_number;

DROP INDEX transactions_miniblock_number_tx_index_idx;
ALTER INDEX transactions_l1_batch_number_idx RENAME TO transactions_block_number_idx;
CREATE INDEX transactions_block_number_tx_index ON transactions (block_number, index_in_block);

ALTER TABLE transactions DROP CONSTRAINT transactions_miniblock_number_fkey;
ALTER TABLE transactions DROP COLUMN miniblock_number;

DROP INDEX miniblocks_l1_batch_number_idx;
DROP TABLE miniblocks;
