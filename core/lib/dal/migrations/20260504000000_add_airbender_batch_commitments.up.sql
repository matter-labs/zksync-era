CREATE TABLE airbender_batch_commitments (
    l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches (number) ON DELETE CASCADE,
    commitment BYTEA NOT NULL,
    aux_data_hash BYTEA NOT NULL,
    events_queue_commitment BYTEA NOT NULL,
    bootloader_initial_content_commitment BYTEA NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
