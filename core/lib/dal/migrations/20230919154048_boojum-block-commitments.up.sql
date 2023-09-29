-- Add up migration script here

ALTER TABLE l1_batches
    ADD COLUMN events_state_queue_commitment BYTEA;

ALTER TABLE l1_batches
    ADD COLUMN bootloader_initial_memory_commitment BYTEA;
