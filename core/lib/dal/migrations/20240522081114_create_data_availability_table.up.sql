CREATE TABLE data_availability
(
    l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,

    -- the BYTEA used for this 2 columns because it is the most generic type
    -- the actual format of blob identifier and inclusion data is defined by the DA client implementation
    blob_id         BYTEA     NOT NULL, -- blob here is an abstract term, unrelated to any DA implementation
    inclusion_data  BYTEA,

    created_at      TIMESTAMP NOT NULL,
    updated_at      TIMESTAMP NOT NULL
);
