ALTER TABLE l1_batches
    DROP COLUMN IF EXISTS events_queue_commitment;
ALTER TABLE l1_batches
    DROP COLUMN IF EXISTS bootloader_initial_content_commitment;

DROP TABLE IF EXISTS events_queue;
