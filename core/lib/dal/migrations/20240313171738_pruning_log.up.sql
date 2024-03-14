CREATE TYPE prune_type AS ENUM ('Soft', 'Hard');

CREATE TABLE pruning_log
(
    pruned_l1_batch  BIGINT,
    pruned_miniblock BIGINT,
    type prune_type,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
