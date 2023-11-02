ALTER TABLE blocks ADD COLUMN IF NOT EXISTS initial_bootloader_heap_content JSONB NOT NULL;
