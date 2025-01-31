DROP TABLE IF EXISTS pruning_log;

DROP TYPE IF EXISTS prune_type;

ALTER TABLE transactions
    ADD FOREIGN KEY (miniblock_number) REFERENCES miniblocks;

ALTER TABLE transactions
    ADD FOREIGN KEY (l1_batch_number) REFERENCES l1_batches;
