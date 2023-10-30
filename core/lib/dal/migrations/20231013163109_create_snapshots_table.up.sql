CREATE TABLE snapshots
(
    l1_batch_number  BIGINT    NOT NULL PRIMARY KEY,
    files            TEXT[]    NOT NULL,
    created_at       TIMESTAMP NOT NULL
);
