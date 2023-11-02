CREATE TABLE IF NOT EXISTS commitments (
    l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches(number) ON DELETE CASCADE,
    events_queue_commitment BYTEA,
    bootloader_initial_content_commitment BYTEA
);

CREATE TABLE IF NOT EXISTS events_queue (
    l1_batch_number BIGINT PRIMARY KEY REFERENCES l1_batches(number) ON DELETE CASCADE,
    serialized_events_queue JSONB NOT NULL
);
