CREATE TABLE miniblocks (
    number BIGSERIAL PRIMARY KEY,
    l1_batch_number BIGINT,
    timestamp BIGINT NOT NULL,
    hash BYTEA NOT NULL,

    l1_tx_count INT NOT NULL,
    l2_tx_count INT NOT NULL,

    base_fee_per_erg NUMERIC(80) NOT NULL,
    ergs_per_pubdata_limit BIGINT NOT NULL,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
CREATE INDEX miniblocks_l1_batch_number_idx ON miniblocks (l1_batch_number);

INSERT INTO miniblocks
    (number, l1_batch_number, timestamp, hash, l1_tx_count, l2_tx_count, base_fee_per_erg, ergs_per_pubdata_limit, created_at, updated_at)
SELECT number, number as l1_batch_number, timestamp, '\x' as hash, l1_tx_count, l2_tx_count, base_fee_per_erg, ergs_per_pubdata_limit,
       now() as created_at, now() as updated_at
FROM blocks;

ALTER TABLE transactions ADD COLUMN miniblock_number BIGINT;
ALTER TABLE transactions ADD CONSTRAINT transactions_miniblock_number_fkey
    FOREIGN KEY (miniblock_number) REFERENCES miniblocks (number);
UPDATE transactions SET miniblock_number = block_number;

DROP INDEX transactions_block_number_tx_index;
ALTER INDEX transactions_block_number_idx RENAME TO transactions_l1_batch_number_idx;
CREATE INDEX transactions_miniblock_number_tx_index_idx ON transactions (miniblock_number, index_in_block);

ALTER TABLE transactions RENAME COLUMN block_number TO l1_batch_number;
ALTER TABLE transactions RENAME CONSTRAINT transactions_block_number_fkey to transactions_l1_batch_number_fkey;

ALTER TABLE contracts DROP CONSTRAINT contracts_block_number_fkey;
ALTER TABLE contracts RENAME COLUMN block_number TO miniblock_number;
ALTER TABLE contracts ADD CONSTRAINT contracts_miniblock_number_fkey
    FOREIGN KEY (miniblock_number) REFERENCES miniblocks (number);

ALTER TABLE events DROP CONSTRAINT events_block_number_fkey;
ALTER TABLE events RENAME COLUMN block_number TO miniblock_number;
ALTER TABLE events ADD CONSTRAINT events_miniblock_number_fkey
    FOREIGN KEY (miniblock_number) REFERENCES miniblocks (number);

ALTER TABLE factory_deps DROP CONSTRAINT factory_deps_block_number_fkey;
ALTER TABLE factory_deps RENAME COLUMN block_number TO miniblock_number;
ALTER TABLE factory_deps ADD CONSTRAINT factory_deps_miniblock_number_fkey
    FOREIGN KEY (miniblock_number) REFERENCES miniblocks (number);

ALTER TABLE storage_logs DROP CONSTRAINT storage_logs_block_number_fkey;
ALTER TABLE storage_logs RENAME COLUMN block_number TO miniblock_number;
ALTER TABLE storage_logs ADD CONSTRAINT storage_logs_miniblock_number_fkey
    FOREIGN KEY (miniblock_number) REFERENCES miniblocks (number);

ALTER TABLE storage_logs_dedup RENAME COLUMN block_number TO l1_batch_number;
ALTER TABLE storage_logs_dedup RENAME CONSTRAINT storage_logs_dedup_block_number_fkey TO storage_logs_dedup_l1_batch_number_fkey;

ALTER TABLE witness_inputs RENAME COLUMN block_number TO l1_batch_number;
ALTER TABLE witness_inputs RENAME CONSTRAINT witness_inputs_block_number_fkey TO witness_inputs_l1_batch_number_fkey;

ALTER TABLE prover_jobs RENAME COLUMN block_number TO l1_batch_number;
ALTER TABLE prover_jobs RENAME CONSTRAINT prover_jobs_block_number_fkey TO prover_jobs_l1_batch_number_fkey;

ALTER TABLE proof RENAME COLUMN block_number TO l1_batch_number;
ALTER TABLE proof RENAME CONSTRAINT proof_block_number_fkey TO proof_l1_batch_number_fkey;

ALTER TABLE blocks RENAME TO l1_batches;
