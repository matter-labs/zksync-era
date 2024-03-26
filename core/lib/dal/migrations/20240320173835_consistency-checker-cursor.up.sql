CREATE TABLE consistency_checker_info
(
    last_processed_l1_batch BIGINT NOT NULL,
    created_at              TIMESTAMP NOT NULL,
    updated_at              TIMESTAMP NOT NULL
);

INSERT INTO consistency_checker_info(last_processed_l1_batch, created_at, updated_at)
VALUES (0, NOW(), NOW());
