-- Add up migration script here

ALTER TABLE l1_batches
    DROP COLUMN events_state_queue_commitment;

ALTER TABLE l1_batches
    DROP COLUMN bootloader_initial_memory_commitment;
