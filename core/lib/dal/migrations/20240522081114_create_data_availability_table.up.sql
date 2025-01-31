CREATE TABLE IF NOT EXISTS data_availability
(
    l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,

    blob_id         TEXT      NOT NULL, -- blob here is an abstract term, unrelated to any DA implementation
    -- the BYTEA used for this column as the most generic type
    -- the actual format of blob identifier and inclusion data is defined by the DA client implementation
    inclusion_data  BYTEA,
    sent_at         TIMESTAMP NOT NULL,

    created_at      TIMESTAMP NOT NULL,
    updated_at      TIMESTAMP NOT NULL
);
