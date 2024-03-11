CREATE TABLE pruning_info
(
    last_soft_pruned_l1_batch  BIGINT,
    last_soft_pruned_miniblock BIGINT,
    last_hard_pruned_l1_batch  BIGINT,
    last_hard_pruned_miniblock BIGINT
);

INSERT INTO pruning_info(last_soft_pruned_l1_batch,
                         last_soft_pruned_miniblock,
                         last_hard_pruned_l1_batch,
                         last_hard_pruned_miniblock)
SELECT CASE
           WHEN (SELECT COUNT(*) FROM snapshot_recovery) = 0 THEN NULL
           ELSE (SELECT l1_batch_number FROM snapshot_recovery) END,
       CASE
           WHEN (SELECT COUNT(*) FROM snapshot_recovery) = 0 THEN NULL
           ELSE (SELECT l1_batch_number FROM snapshot_recovery) END,
       NULL,
       NULL
