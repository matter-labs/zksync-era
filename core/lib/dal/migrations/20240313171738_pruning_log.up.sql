CREATE TYPE prune_type AS ENUM ('Soft', 'Hard');

CREATE TABLE pruning_log
(
    pruned_l1_batch  BIGINT NOT NULL,
    pruned_miniblock BIGINT NOT NULL,
    type prune_type NOT NULL,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    PRIMARY KEY (type, pruned_l1_batch)
);

