ALTER TABLE events_queue ADD COLUMN IF NOT EXISTS serialized_events_queue_bytea BYTEA;
