CREATE TABLE protective_reads (
    l1_batch_number BIGINT REFERENCES l1_batches (number) ON DELETE CASCADE,
    address BYTEA NOT NULL,
    key BYTEA NOT NULL,

    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    PRIMARY KEY (address, key, l1_batch_number)
);

CREATE INDEX protective_reads_l1_batch_number_index ON protective_reads (l1_batch_number);
