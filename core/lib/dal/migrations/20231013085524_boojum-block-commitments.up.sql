ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS events_queue_commitment BYTEA;
ALTER TABLE l1_batches
    ADD COLUMN IF NOT EXISTS bootloader_initial_content_commitment BYTEA;

CREATE TABLE IF NOT EXISTS events_queue (
    l1_batch_number BIGINT PRIMARY KEY,
    serialized_events_queue JSONB NOT NULL
);
