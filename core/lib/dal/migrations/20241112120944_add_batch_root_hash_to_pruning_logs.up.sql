-- nullable for backward compatibility
ALTER TABLE pruning_log
    ADD COLUMN pruned_l1_batch_root_hash BYTEA DEFAULT NULL;
